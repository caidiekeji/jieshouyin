import { readJson, json, str, clientIp, assertSameOrigin, HttpError, withSetCookies, clampInt } from '../http.js';
import { getSettings, saveSettings, nowIso, audit, isValidDateString } from '../db.js';
import { hashPassword, verifyPassword, createSession, destroySession, assertLoginAllowed, recordLoginFailure, clearLoginFailures, PASSWORD_RULE, sessionHash } from '../auth.js';
import { verifyTotp, generateTotpSecret, otpauthUrl } from '../totp.js';
import { createLicense, grantFreeLicense } from '../services.js';
import { isExpired, licenseTypeFor } from '../license.js';
import { normalizeEmail, isValidEmail, sendMail, testMail } from '../mail.js';
import { recordEmailEvent } from '../email-auth.js';
import { sendExpiryReminders } from '../reminders.js';
const USERNAME_RULE = /^[A-Za-z0-9_.-]{3,30}$/;
const CST_OFFSET = 8 * 3600_000;
function cstDayKey(iso) {
    return new Date(Date.parse(iso) + CST_OFFSET).toISOString().slice(0, 10);
}
function cstStartOfTodayIso() {
    const local = new Date(Date.now() + CST_OFFSET);
    local.setUTCHours(0, 0, 0, 0);
    return new Date(local.getTime() - CST_OFFSET).toISOString();
}
function pageParams(url) {
    const page = clampInt(url.searchParams.get('page'), 1, 1, 10_000);
    const size = clampInt(url.searchParams.get('size'), 20, 1, 100);
    return {
        page,
        size,
        offset: (page - 1) * size
    };
}
function likeParam(q) {
    return `%${q.replace(/%/g, '').slice(0, 60)}%`;
}
function publicAdmin(a) {
    return {
        id: a.id,
        username: a.username,
        display_name: a.display_name,
        role: a.role,
        status: a.status,
        totp_enabled: Boolean(Number(a.totp_enabled || 0)),
        created_at: a.created_at,
        last_login_at: a.last_login_at
    };
}
export async function requireAdmin(env, req, minRole = 'viewer') {
    const tokenHash = await sessionHash(req, 'admin');
    if (!tokenHash) throw new HttpError(401, '请先登录管理后台');
    const admin = await env.DB.prepare(`SELECT a.* FROM sessions s
       JOIN admin_users a ON a.id = s.principal_id
      WHERE s.token = ? AND s.principal = 'admin' AND s.expires_at > ?`).bind(tokenHash, nowIso()).first();
    if (!admin) throw new HttpError(401, '登录状态已失效，请重新登录');
    if (admin.status === 'disabled') throw new HttpError(403, '管理员账号已停用');
    if (minRole === 'super' && admin.role !== 'super') throw new HttpError(403, '仅超级管理员可执行此操作');
    if (minRole === 'admin' && admin.role === 'viewer') throw new HttpError(403, '只读账号无操作权限');
    return admin;
}
export async function handleBootstrap(req, env) {
    assertSameOrigin(req);
    const body = await readJson(req);
    const count = await env.DB.prepare('SELECT COUNT(*) AS n FROM admin_users').first();
    if ((count?.n ?? 0) > 0) throw new HttpError(403, '系统已完成初始化，请直接登录');
    const setupToken = String(body.setup_token ?? '');
    if (!env.SETUP_TOKEN) throw new HttpError(403, '服务端未配置 SETUP_TOKEN，请先执行 wrangler secret put SETUP_TOKEN');
    if (setupToken !== env.SETUP_TOKEN) throw new HttpError(403, '初始化口令不正确');
    const username = str(body.username, 30);
    const password = String(body.password ?? '');
    if (!USERNAME_RULE.test(username)) throw new HttpError(400, '用户名需为 3-30 位字母、数字、_ . -');
    if (!PASSWORD_RULE.test(password)) throw new HttpError(400, '密码需 8-64 位，且至少包含一个字母或数字');
    const result = await env.DB.prepare(`INSERT INTO admin_users (username, password_hash, display_name, role, status, created_at)
     VALUES (?, ?, ?, 'super', 'active', ?)`).bind(username, await hashPassword(password), str(body.display_name, 30) || username, nowIso()).run();
    const id = Number(result.meta.last_row_id ?? 0);
    await audit(env, `admin:${id}`, 'admin.bootstrap', username, '初始化超级管理员', clientIp(req));
    const session = await createSession(env, req, 'admin', id);
    return withSetCookies(json({
        ok: true,
        username,
        role: 'super'
    }), [
        session.cookie
    ]);
}
export async function handleAdminLogin(req, env) {
    assertSameOrigin(req);
    const body = await readJson(req);
    const username = str(body.username, 30);
    const password = String(body.password ?? '');
    if (!username || !password) throw new HttpError(400, '请填写用户名与密码');
    const ip = clientIp(req);
    await assertLoginAllowed(env, 'admin', username, ip);
    const admin = await env.DB.prepare('SELECT * FROM admin_users WHERE username = ?').bind(username).first();
    if (!admin || !await verifyPassword(password, admin.password_hash)) {
        await recordLoginFailure(env, 'admin', username, ip);
        throw new HttpError(401, '用户名或密码不正确');
    }
    if (admin.status === 'disabled') throw new HttpError(403, '管理员账号已停用');
    if (admin.totp_enabled) {
        const code = str(body.totp_code, 8);
        if (!code) throw new HttpError(401, '请输入动态验证码');
        if (!await verifyTotp(admin.totp_secret, code)) {
            await recordLoginFailure(env, 'admin', username, ip);
            throw new HttpError(401, '动态验证码不正确');
        }
    }
    await clearLoginFailures(env, 'admin', username, ip);
    await env.DB.prepare('UPDATE admin_users SET last_login_at = ? WHERE id = ?').bind(nowIso(), admin.id).run();
    await audit(env, `admin:${admin.id}`, 'admin.login', admin.username, '登录管理后台', ip);
    const session = await createSession(env, req, 'admin', admin.id);
    return withSetCookies(json({
        admin: publicAdmin(admin)
    }), [
        session.cookie
    ]);
}
export async function handleAdminLogout(req, env) {
    assertSameOrigin(req);
    return withSetCookies(json({
        ok: true
    }), [
        await destroySession(env, req, 'admin')
    ]);
}
export async function handleAdminMe(req, env) {
    const admin = await requireAdmin(env, req);
    return json({
        admin: publicAdmin(admin)
    });
}
export async function handleStats(req, env) {
    await requireAdmin(env, req);
    const now = nowIso();
    const todayStart = cstStartOfTodayIso();
    const d7 = new Date(Date.now() - 7 * 86_400_000).toISOString();
    const d30 = new Date(Date.now() - 30 * 86_400_000).toISOString();
    const d24 = new Date(Date.now() - 86_400_000).toISOString();
    const hourAgo = new Date(Date.now() - 3600_000).toISOString();
    const d14 = new Date(Date.now() - 13 * 86_400_000);
    d14.setUTCHours(0, 0, 0, 0);
    const soon = new Date(Date.now() + 30 * 86_400_000).toISOString();
    const todayKey = cstDayKey(now);
    const key7 = cstDayKey(new Date(Date.now() - 6 * 86_400_000).toISOString());
    const summarySql = `
    SELECT
      (SELECT COUNT(*) FROM users) AS users_total,
      (SELECT COUNT(*) FROM users WHERE status = 'active') AS users_active,
      (SELECT COUNT(*) FROM users WHERE created_at >= ?) AS users_new7,
      (SELECT COUNT(*) FROM licenses) AS lic_total,
      (SELECT COUNT(*) FROM licenses WHERE status = 'active' AND (valid_until IS NULL OR valid_until > ?)) AS lic_active,
      (SELECT COUNT(*) FROM licenses WHERE status = 'active' AND valid_until IS NOT NULL AND valid_until <= ?) AS lic_expired,
      (SELECT COUNT(*) FROM licenses WHERE status = 'revoked') AS lic_revoked,
      (SELECT COUNT(*) FROM licenses WHERE status = 'active' AND valid_until IS NOT NULL AND valid_until > ? AND valid_until <= ?) AS lic_expiring,
      (SELECT COUNT(*) FROM activations WHERE result = 'ok') AS act_total,
      (SELECT COUNT(*) FROM activations WHERE result = 'ok' AND created_at >= ?) AS act_today,
      (SELECT COUNT(*) FROM activations WHERE result = 'ok' AND created_at >= ?) AS act_7,
      (SELECT COUNT(*) FROM devices) AS devices_total,
      (SELECT COUNT(*) FROM devices WHERE last_activated_at >= ?) AS devices_30,
      (SELECT COUNT(*) FROM devices WHERE last_activated_at >= ?) AS devices_24,
      (SELECT COUNT(*) FROM devices WHERE last_activated_at >= ?) AS devices_7,
      (SELECT COUNT(*) FROM activations WHERE result = 'error' AND created_at >= ?) AS fail_1h,
      (SELECT COUNT(*) FROM unbind_requests WHERE status = 'pending') AS pending_requests,
      (SELECT COUNT(*) FROM feedback WHERE status = 'open') AS open_feedback,
      (SELECT COALESCE(SUM(n), 0) FROM page_views) AS views_total,
      (SELECT COALESCE(SUM(n), 0) FROM page_views WHERE day = ?) AS views_today,
      (SELECT COALESCE(SUM(n), 0) FROM page_views WHERE day >= ?) AS views_7
  `;
    const [agg, dailyRes, recentActRes, recentUsersRes, topLicRes, regionsRes, pagesRes] = await Promise.all([
        env.DB.prepare(summarySql).bind(d7, now, now, now, soon, todayStart, d7, d30, d24, d7, hourAgo, todayKey, key7).first(),
        env.DB.prepare(`SELECT date(created_at, '+8 hours') AS day, COUNT(*) AS n
         FROM activations WHERE result = 'ok' AND created_at >= ?
        GROUP BY day ORDER BY day`).bind(d14.toISOString()).all(),
        env.DB.prepare(`SELECT a.id, a.license_key, a.store_id, a.device_fingerprint, a.created_at, a.result, a.reason,
              l.customer_name, u.account
         FROM activations a
         LEFT JOIN licenses l ON l.id = a.license_id
         LEFT JOIN users u ON u.id = a.user_id
        ORDER BY a.id DESC LIMIT 10`).all(),
        env.DB.prepare(`SELECT id, account, store_name, customer_name, created_at
         FROM users ORDER BY id DESC LIMIT 8`).all(),
        env.DB.prepare(`SELECT a.license_id, a.license_key, l.store_id,
              COUNT(*) AS n, COUNT(DISTINCT a.device_fingerprint) AS devices
         FROM activations a LEFT JOIN licenses l ON l.id = a.license_id
        WHERE a.result = 'ok' GROUP BY a.license_id, a.license_key, l.store_id
        ORDER BY n DESC LIMIT 6`).all(),
        env.DB.prepare(`SELECT CASE WHEN country = '' THEN '未记录' ELSE country END AS name, COUNT(*) AS n
         FROM devices GROUP BY name ORDER BY n DESC LIMIT 10`).all(),
        env.DB.prepare(`SELECT path, SUM(n) AS n FROM page_views GROUP BY path ORDER BY n DESC LIMIT 6`).all()
    ]);
    const daily = dailyRes.results ?? [];
    const series = [];
    for(let i = 13; i >= 0; i--){
        const day = cstDayKey(new Date(Date.now() - i * 86_400_000).toISOString());
        series.push({
            day,
            n: daily.find((d)=>d.day === day)?.n ?? 0
        });
    }
    const n = (key)=>Number(agg?.[key] ?? 0);
    return json({
        users: {
            total: n('users_total'),
            active: n('users_active'),
            new7: n('users_new7')
        },
        licenses: {
            total: n('lic_total'),
            active: n('lic_active'),
            expired: n('lic_expired'),
            revoked: n('lic_revoked'),
            expiring30: n('lic_expiring')
        },
        activations: {
            total: n('act_total'),
            today: n('act_today'),
            last7: n('act_7'),
            fail1h: n('fail_1h')
        },
        devices: {
            total: n('devices_total'),
            active24: n('devices_24'),
            active7: n('devices_7'),
            active30: n('devices_30')
        },
        ops: {
            pending_requests: n('pending_requests'),
            open_feedback: n('open_feedback')
        },
        views: {
            total: n('views_total'),
            today: n('views_today'),
            last7: n('views_7'),
            top: pagesRes.results ?? []
        },
        regions: regionsRes.results ?? [],
        series,
        recent_activations: recentActRes.results ?? [],
        recent_users: recentUsersRes.results ?? [],
        top_licenses: topLicRes.results ?? []
    });
}
export async function handleUsers(req, env, url) {
    await requireAdmin(env, req);
    const { page, size, offset } = pageParams(url);
    const keyword = str(url.searchParams.get('q'), 60);
    const status = str(url.searchParams.get('status'), 20);
    let where = '1 = 1';
    const binds = [];
    if (keyword) {
        where += ' AND (u.account LIKE ? OR u.store_name LIKE ? OR u.customer_name LIKE ? OR u.contact LIKE ? OR u.email LIKE ?)';
        const like = likeParam(keyword);
        binds.push(like, like, like, like, like);
    }
    if (status === 'active' || status === 'disabled') {
        where += ' AND u.status = ?';
        binds.push(status);
    }
    const total = await env.DB.prepare(`SELECT COUNT(*) AS n FROM users u WHERE ${where}`).bind(...binds).first();
    const { results } = await env.DB.prepare(`SELECT u.id, u.account, u.store_name, u.customer_name, u.contact, u.email, u.email_verified_at, u.status, u.note,
            u.created_at, u.last_login_at,
            (SELECT COUNT(*) FROM licenses l WHERE l.user_id = u.id) AS license_count,
            (SELECT COUNT(*) FROM activations a WHERE a.user_id = u.id AND a.result = 'ok') AS activation_count
       FROM users u WHERE ${where}
      ORDER BY u.id DESC LIMIT ? OFFSET ?`).bind(...binds, size, offset).all();
    return json({
        total: total?.n ?? 0,
        page,
        size,
        items: results ?? []
    });
}
export async function handleUserDetail(req, env, id) {
    await requireAdmin(env, req);
    const user = await env.DB.prepare('SELECT * FROM users WHERE id = ?').bind(id).first();
    if (!user) throw new HttpError(404, '用户不存在');
    const { results: licenses } = await env.DB.prepare(`SELECT l.*, (SELECT COUNT(*) FROM devices d WHERE d.license_id = l.id) AS device_count
       FROM licenses l WHERE l.user_id = ? ORDER BY l.id DESC`).bind(id).all();
    const { results: devices } = await env.DB.prepare(`SELECT d.* FROM devices d JOIN licenses l ON l.id = d.license_id
      WHERE l.user_id = ? ORDER BY d.last_activated_at DESC`).bind(id).all();
    const { results: activations } = await env.DB.prepare(`SELECT * FROM activations WHERE user_id = ? ORDER BY id DESC LIMIT 20`).bind(id).all();
    return json({
        user: {
            ...publicAdminSafe(user),
            account: user.account,
            store_name: user.store_name,
            customer_name: user.customer_name,
            contact: user.contact,
            status: user.status,
            note: user.note
        },
        licenses: licenses ?? [],
        devices: devices ?? [],
        activations: activations ?? []
    });
}
function publicAdminSafe(user) {
    return {
        id: user.id,
        created_at: user.created_at,
        last_login_at: user.last_login_at
    };
}
export async function handleUserUpdate(req, env, id) {
    assertSameOrigin(req);
    const admin = await requireAdmin(env, req, 'admin');
    const user = await env.DB.prepare('SELECT * FROM users WHERE id = ?').bind(id).first();
    if (!user) throw new HttpError(404, '用户不存在');
    const body = await readJson(req);
    const patch = {};
    if (body.status !== undefined) {
        const status = String(body.status);
        if (status !== 'active' && status !== 'disabled') throw new HttpError(400, 'status 只能是 active 或 disabled');
        patch.status = status;
    }
    if (body.store_name !== undefined) patch.store_name = str(body.store_name, 40);
    if (body.customer_name !== undefined) patch.customer_name = str(body.customer_name, 30);
    if (body.contact !== undefined) patch.contact = str(body.contact, 60);
    if (body.note !== undefined) patch.note = str(body.note, 200);
    if (Object.keys(patch).length === 0) throw new HttpError(400, '没有需要更新的字段');
    const sets = Object.keys(patch).map((k)=>`${k} = ?`).join(', ');
    await env.DB.prepare(`UPDATE users SET ${sets} WHERE id = ?`).bind(...Object.values(patch), id).run();
    await audit(env, `admin:${admin.id}`, 'user.update', user.account, JSON.stringify(patch), clientIp(req));
    return json({
        ok: true
    });
}
export async function handleUserPassword(req, env, id) {
    assertSameOrigin(req);
    const admin = await requireAdmin(env, req, 'admin');
    const user = await env.DB.prepare('SELECT * FROM users WHERE id = ?').bind(id).first();
    if (!user) throw new HttpError(404, '用户不存在');
    const body = await readJson(req);
    const password = String(body.password ?? '');
    if (!PASSWORD_RULE.test(password)) throw new HttpError(400, '密码需 8-64 位，且至少包含一个字母或数字');
    await env.DB.prepare('UPDATE users SET password_hash = ? WHERE id = ?').bind(await hashPassword(password), id).run();
    await env.DB.prepare("DELETE FROM sessions WHERE principal = 'user' AND principal_id = ?").bind(id).run();
    await audit(env, `admin:${admin.id}`, 'user.password.reset', user.account, '管理员重置用户密码', clientIp(req));
    return json({
        ok: true
    });
}
export async function handleUserDelete(req, env, id) {
    assertSameOrigin(req);
    const admin = await requireAdmin(env, req, 'super');
    const user = await env.DB.prepare('SELECT * FROM users WHERE id = ?').bind(id).first();
    if (!user) throw new HttpError(404, '用户不存在');
    await env.DB.batch([
        env.DB.prepare("DELETE FROM sessions WHERE principal = 'user' AND principal_id = ?").bind(id),
        env.DB.prepare('DELETE FROM users WHERE id = ?').bind(id)
    ]);
    await audit(env, `admin:${admin.id}`, 'user.delete', user.account, `删除用户 ${user.store_name}`, clientIp(req));
    return json({
        ok: true
    });
}
export async function handleLicenses(req, env, url) {
    await requireAdmin(env, req);
    const { page, size, offset } = pageParams(url);
    const keyword = str(url.searchParams.get('q'), 60);
    const status = str(url.searchParams.get('status'), 20);
    const now = nowIso();
    let where = '1 = 1';
    const binds = [];
    if (keyword) {
        where += ' AND (l.license_key LIKE ? OR l.license_id LIKE ? OR l.store_id LIKE ? OR l.customer_name LIKE ? OR u.account LIKE ?)';
        const like = likeParam(keyword);
        binds.push(like, like, like, like, like);
    }
    if (status === 'active') {
        where += ' AND l.status = ? AND (l.valid_until IS NULL OR l.valid_until > ?)';
        binds.push('active', now);
    } else if (status === 'expired') {
        where += " AND l.status = 'active' AND l.valid_until IS NOT NULL AND l.valid_until <= ?";
        binds.push(now);
    } else if (status === 'revoked') {
        where += ' AND l.status = ?';
        binds.push('revoked');
    }
    const total = await env.DB.prepare(`SELECT COUNT(*) AS n FROM licenses l LEFT JOIN users u ON u.id = l.user_id WHERE ${where}`).bind(...binds).first();
    const { results } = await env.DB.prepare(`SELECT l.*, u.account, u.store_name AS user_store,
            (SELECT COUNT(*) FROM devices d WHERE d.license_id = l.id) AS device_count,
            (SELECT COUNT(*) FROM activations a WHERE a.license_id = l.id AND a.result = 'ok') AS activation_count
       FROM licenses l LEFT JOIN users u ON u.id = l.user_id
      WHERE ${where}
      ORDER BY l.id DESC LIMIT ? OFFSET ?`).bind(...binds, size, offset).all();
    const items = (results ?? []).map((row)=>({
            ...row,
            expired: isExpired(row.valid_until)
        }));
    return json({
        total: total?.n ?? 0,
        page,
        size,
        items
    });
}
export async function handleLicenseCreate(req, env) {
    assertSameOrigin(req);
    const admin = await requireAdmin(env, req, 'admin');
    const body = await readJson(req);
    const account = str(body.account, 40);
    let user = null;
    if (account) {
        user = await env.DB.prepare('SELECT * FROM users WHERE account = ?').bind(account).first();
        if (!user) throw new HttpError(404, `用户 ${account} 不存在`);
    }
    const days = clampInt(body.days, 365, 0, 3650);
    const maxDevices = clampInt(body.max_devices, 1, 1, 20);
    const storeId = str(body.store_id, 60) || user?.store_name || '';
    const customerName = str(body.customer_name, 30) || user?.customer_name || '';
    if (!storeId) throw new HttpError(400, '请填写门店名称');
    const license = await createLicense(env, {
        userId: user?.id ?? null,
        storeId,
        customerName,
        contact: str(body.contact, 60) || user?.contact || '',
        days,
        maxDevices,
        source: 'admin',
        note: str(body.note, 200),
        adminId: admin.id
    });
    await audit(env, `admin:${admin.id}`, 'license.issue', license.license_key, `${storeId} / ${customerName} / ${days === 0 ? '永久' : `${days} 天`} / ${maxDevices} 台`, clientIp(req));
    return json({
        license
    });
}
export async function handleLicenseDetail(req, env, id) {
    await requireAdmin(env, req);
    const license = await env.DB.prepare('SELECT * FROM licenses WHERE id = ?').bind(id).first();
    if (!license) throw new HttpError(404, '授权不存在');
    const user = license.user_id ? await env.DB.prepare('SELECT id, account, store_name, customer_name, contact FROM users WHERE id = ?').bind(license.user_id).first() : null;
    const { results: devices } = await env.DB.prepare('SELECT * FROM devices WHERE license_id = ? ORDER BY last_activated_at DESC').bind(id).all();
    const { results: activations } = await env.DB.prepare('SELECT * FROM activations WHERE license_id = ? ORDER BY id DESC LIMIT 20').bind(id).all();
    return json({
        license: {
            ...license,
            expired: isExpired(license.valid_until)
        },
        user,
        devices: devices ?? [],
        activations: activations ?? []
    });
}
export async function handleLicenseUpdate(req, env, id) {
    assertSameOrigin(req);
    const admin = await requireAdmin(env, req, 'admin');
    const license = await env.DB.prepare('SELECT * FROM licenses WHERE id = ?').bind(id).first();
    if (!license) throw new HttpError(404, '授权不存在');
    const body = await readJson(req);
    const sets = [];
    const binds = [];
    if (body.status !== undefined) {
        const status = String(body.status);
        if (status !== 'active' && status !== 'revoked') throw new HttpError(400, 'status 只能是 active 或 revoked');
        sets.push('status = ?');
        binds.push(status);
    }
    if (body.valid_until !== undefined) {
        const raw = str(body.valid_until, 40);
        if (!raw) {
            sets.push('valid_until = ?', 'license_type = ?');
            binds.push(null, 'PERPETUAL');
        } else {
            const normalized = raw.length <= 10 ? `${raw}T23:59:59Z` : raw;
            if (!isValidDateString(normalized)) throw new HttpError(400, '到期时间格式不正确');
            sets.push('valid_until = ?', 'license_type = ?');
            binds.push(new Date(normalized).toISOString().replace(/\.\d{3}Z$/, 'Z'), 'ANNUAL');
        }
    }
    if (body.max_devices !== undefined) {
        sets.push('max_devices = ?');
        binds.push(clampInt(body.max_devices, license.max_devices, 1, 20));
    }
    if (body.note !== undefined) {
        sets.push('note = ?');
        binds.push(str(body.note, 200));
    }
    if (sets.length === 0) throw new HttpError(400, '没有需要更新的字段');
    await env.DB.prepare(`UPDATE licenses SET ${sets.join(', ')} WHERE id = ?`).bind(...binds, id).run();
    await audit(env, `admin:${admin.id}`, 'license.update', license.license_key, JSON.stringify(body), clientIp(req));
    return json({
        ok: true
    });
}
export async function handleLicenseBatch(req, env) {
    assertSameOrigin(req);
    const admin = await requireAdmin(env, req, 'admin');
    const body = await readJson(req);
    const action = String(body.action || '');
    const ids = Array.isArray(body.ids) ? body.ids.map((v)=>Number(v)).filter((v)=>Number.isInteger(v) && v > 0).slice(0, 100) : [];
    if (!ids.length) throw new HttpError(400, '请选择至少一条授权');
    if (![
        'revoke',
        'restore',
        'extend'
    ].includes(action)) throw new HttpError(400, '批量操作不合法');
    if (action === 'extend') {
        const days = clampInt(body.days, 365, 1, 3650);
        for (const id of ids){
            const license = await env.DB.prepare('SELECT * FROM licenses WHERE id = ?').bind(id).first();
            if (!license) continue;
            const base = license.valid_until && Date.parse(license.valid_until) > Date.now() ? Date.parse(license.valid_until) : Date.now();
            const newUntil = new Date(base + days * 86_400_000).toISOString().replace(/\.\d{3}Z$/, 'Z');
            await env.DB.prepare('UPDATE licenses SET valid_until = ?, license_type = ? WHERE id = ?').bind(newUntil, licenseTypeFor(days), id).run();
        }
    } else {
        const status = action === 'revoke' ? 'revoked' : 'active';
        await env.DB.batch(ids.map((id)=>env.DB.prepare('UPDATE licenses SET status = ? WHERE id = ?').bind(status, id)));
    }
    await audit(env, `admin:${admin.id}`, `license.batch.${action}`, String(ids.length), `批量${action} ${ids.length} 条授权`, clientIp(req));
    return json({
        ok: true,
        count: ids.length
    });
}
export async function handleLicenseBatchCreate(req, env) {
    assertSameOrigin(req);
    const admin = await requireAdmin(env, req, 'admin');
    const body = await readJson(req);
    const quantity = clampInt(body.quantity, 1, 1, 50);
    const days = clampInt(body.days, 365, 0, 3650);
    const maxDevices = clampInt(body.max_devices, 1, 1, 20);
    const storeId = str(body.store_id, 60);
    const customerName = str(body.customer_name, 30);
    const contact = str(body.contact, 60);
    const note = str(body.note, 200);
    const created = [];
    for(let i = 0; i < quantity; i++){
        created.push(await createLicense(env, {
            userId: null,
            storeId,
            customerName,
            contact,
            days,
            maxDevices,
            source: 'admin',
            note,
            adminId: admin.id
        }));
    }
    await audit(env, `admin:${admin.id}`, 'license.batch.create', String(quantity), `批量签发 ${quantity} 条授权`, clientIp(req));
    return json({
        ok: true,
        licenses: created
    });
}
export async function handleLicenseExtend(req, env, id) {
    assertSameOrigin(req);
    const admin = await requireAdmin(env, req, 'admin');
    const license = await env.DB.prepare('SELECT * FROM licenses WHERE id = ?').bind(id).first();
    if (!license) throw new HttpError(404, '授权不存在');
    const body = await readJson(req);
    const days = clampInt(body.days, 365, 1, 3650);
    const base = license.valid_until && Date.parse(license.valid_until) > Date.now() ? Date.parse(license.valid_until) : Date.now();
    const newUntil = new Date(base + days * 86_400_000).toISOString().replace(/\.\d{3}Z$/, 'Z');
    await env.DB.prepare('UPDATE licenses SET valid_until = ?, license_type = ? WHERE id = ?').bind(newUntil, licenseTypeFor(days), id).run();
    await audit(env, `admin:${admin.id}`, 'license.extend', license.license_key, `续期 ${days} 天至 ${newUntil.slice(0, 10)}`, clientIp(req));
    return json({
        ok: true,
        valid_until: newUntil
    });
}
export async function handleDeviceDelete(req, env, licenseId, deviceId) {
    assertSameOrigin(req);
    const admin = await requireAdmin(env, req, 'admin');
    const device = await env.DB.prepare('SELECT * FROM devices WHERE id = ? AND license_id = ?').bind(deviceId, licenseId).first();
    if (!device) throw new HttpError(404, '设备不存在');
    await env.DB.prepare('DELETE FROM devices WHERE id = ?').bind(deviceId).run();
    await audit(env, `admin:${admin.id}`, 'device.unbind', String(licenseId), device.fingerprint, clientIp(req));
    return json({
        ok: true
    });
}
export async function handleActivations(req, env, url) {
    await requireAdmin(env, req);
    const { page, size, offset } = pageParams(url);
    const keyword = str(url.searchParams.get('q'), 60);
    const result = str(url.searchParams.get('result'), 20);
    let where = '1 = 1';
    const binds = [];
    if (keyword) {
        where += ' AND (a.license_key LIKE ? OR a.store_id LIKE ? OR a.device_fingerprint LIKE ? OR u.account LIKE ?)';
        const like = likeParam(keyword);
        binds.push(like, like, like, like);
    }
    if (result === 'ok' || result === 'error') {
        where += ' AND a.result = ?';
        binds.push(result);
    }
    const total = await env.DB.prepare(`SELECT COUNT(*) AS n FROM activations a LEFT JOIN users u ON u.id = a.user_id WHERE ${where}`).bind(...binds).first();
    const { results } = await env.DB.prepare(`SELECT a.*, u.account, l.store_id AS license_store, l.customer_name
       FROM activations a
       LEFT JOIN users u ON u.id = a.user_id
       LEFT JOIN licenses l ON l.id = a.license_id
      WHERE ${where}
      ORDER BY a.id DESC LIMIT ? OFFSET ?`).bind(...binds, size, offset).all();
    return json({
        total: total?.n ?? 0,
        page,
        size,
        items: results ?? []
    });
}
export async function handleSettingsGet(req, env) {
    await requireAdmin(env, req);
    return json({
        settings: await getSettings(env)
    });
}
export async function handleSettingsUpdate(req, env) {
    assertSameOrigin(req);
    const admin = await requireAdmin(env, req, 'super');
    const body = await readJson(req);
    const patch = {};
    const boolField = (key, target)=>{
        if (body[key] !== undefined) patch[target] = body[key] ? '1' : '0';
    };
    boolField('registration_open', 'registration_open');
    boolField('free_license_enabled', 'free_license_enabled');
    boolField('email_login_code', 'email_login_code');
    boolField('expiry_reminder', 'expiry_reminder');
    if (body.expiry_reminder_days !== undefined) {
        patch.expiry_reminder_days = String(clampInt(body.expiry_reminder_days, 7, 1, 30));
    }
    if (body.free_license_days !== undefined) {
        patch.free_license_days = String(clampInt(body.free_license_days, 365, 0, 3650));
    }
    if (body.free_max_devices !== undefined) {
        patch.free_max_devices = String(clampInt(body.free_max_devices, 1, 1, 20));
    }
    if (body.grace_days !== undefined) {
        patch.grace_days = String(clampInt(body.grace_days, 7, 0, 90));
    }
    if (body.site_notice !== undefined) patch.site_notice = str(body.site_notice, 200);
    if (body.site_name !== undefined) patch.site_name = str(body.site_name, 40);
    if (body.site_keywords !== undefined) patch.site_keywords = str(body.site_keywords, 200);
    if (body.site_description !== undefined) patch.site_description = str(body.site_description, 200);
    if (body.contact_email !== undefined) patch.contact_email = str(body.contact_email, 120);
    if (body.contact_phone !== undefined) patch.contact_phone = str(body.contact_phone, 40);
    if (body.contact_address !== undefined) patch.contact_address = str(body.contact_address, 200);
    if (body.icp_no !== undefined) patch.icp_no = str(body.icp_no, 80);
    await saveSettings(env, patch);
    await audit(env, `admin:${admin.id}`, 'settings.update', 'settings', JSON.stringify(patch), clientIp(req));
    return json({
        settings: await getSettings(env)
    });
}
export async function handleEmailStatus(req, env) {
    await requireAdmin(env, req);
    const settings = await getSettings(env);
    const last = await env.DB.prepare("SELECT kind, email, status, detail, created_at FROM email_events ORDER BY id DESC LIMIT 1").first();
    return json({
        configured: Boolean(env.RESEND_API_KEY),
        from: env.MAIL_FROM || 'Checkout-R <admin@lequ.pw>',
        reply_to: env.MAIL_REPLY_TO || '',
        email_login_code: settings.email_login_code,
        expiry_reminder: settings.expiry_reminder,
        expiry_reminder_days: settings.expiry_reminder_days,
        last_event: last ?? null
    });
}
export async function handleEmailRunReminders(req, env) {
    assertSameOrigin(req);
    const admin = await requireAdmin(env, req, 'super');
    const result = await sendExpiryReminders(env);
    await audit(env, `admin:${admin.id}`, 'email.reminder_run', 'licenses', JSON.stringify(result), clientIp(req));
    return json({
        ok: true,
        ...result
    });
}
export async function handleEmailTest(req, env) {
    assertSameOrigin(req);
    const admin = await requireAdmin(env, req, 'super');
    const body = await readJson(req);
    const to = normalizeEmail(body.to);
    if (!isValidEmail(to)) throw new HttpError(400, '请输入正确的测试收件邮箱');
    const minuteKey = `admin_test:${admin.id}:${Math.floor(Date.now() / 60_000)}`;
    const recent = await env.DB.prepare('SELECT id FROM email_events WHERE dedupe_key = ?').bind(minuteKey).first();
    if (recent) throw new HttpError(429, '每分钟只能发送一封测试邮件');
    const mail = testMail();
    try {
        const result = await sendMail(env, {
            to,
            ...mail
        });
        await recordEmailEvent(env, 'admin_test', to, null, 'ok', '后台测试邮件', minuteKey);
        await audit(env, `admin:${admin.id}`, 'email.test', to, '发送测试邮件', clientIp(req));
        return json({
            ok: true,
            id: result.id
        });
    } catch (e) {
        await recordEmailEvent(env, 'admin_test', to, null, 'error', String(e).slice(0, 200), minuteKey);
        throw e;
    }
}
export async function handleAdmins(req, env) {
    await requireAdmin(env, req);
    const { results } = await env.DB.prepare('SELECT id, username, display_name, role, status, created_at, last_login_at FROM admin_users ORDER BY id').all();
    return json({
        admins: results ?? []
    });
}
export async function handleAdminCreate(req, env) {
    assertSameOrigin(req);
    const admin = await requireAdmin(env, req, 'super');
    const body = await readJson(req);
    const username = str(body.username, 30);
    const password = String(body.password ?? '');
    const role = String(body.role ?? 'admin');
    if (!USERNAME_RULE.test(username)) throw new HttpError(400, '用户名需为 3-30 位字母、数字、_ . -');
    if (!PASSWORD_RULE.test(password)) throw new HttpError(400, '密码需 8-64 位，且至少包含一个字母或数字');
    if (role !== 'admin' && role !== 'viewer' && role !== 'super') throw new HttpError(400, '角色不正确');
    const dup = await env.DB.prepare('SELECT id FROM admin_users WHERE username = ?').bind(username).first();
    if (dup) throw new HttpError(409, '该用户名已存在');
    await env.DB.prepare(`INSERT INTO admin_users (username, password_hash, display_name, role, status, created_at)
     VALUES (?, ?, ?, ?, 'active', ?)`).bind(username, await hashPassword(password), str(body.display_name, 30) || username, role, nowIso()).run();
    await audit(env, `admin:${admin.id}`, 'admin.create', username, `角色 ${role}`, clientIp(req));
    return json({
        ok: true
    });
}
export async function handleAdminUpdate(req, env, id) {
    assertSameOrigin(req);
    const actor = await requireAdmin(env, req, 'super');
    const target = await env.DB.prepare('SELECT * FROM admin_users WHERE id = ?').bind(id).first();
    if (!target) throw new HttpError(404, '管理员不存在');
    const body = await readJson(req);
    const sets = [];
    const binds = [];
    if (body.display_name !== undefined) {
        sets.push('display_name = ?');
        binds.push(str(body.display_name, 30));
    }
    if (body.role !== undefined) {
        const role = String(body.role);
        if (role !== 'super' && role !== 'admin' && role !== 'viewer') throw new HttpError(400, '角色不正确');
        if (target.role === 'super' && role !== 'super') {
            const supers = await env.DB.prepare("SELECT COUNT(*) AS n FROM admin_users WHERE role = 'super' AND status = 'active' AND id <> ?").bind(id).first();
            if ((supers?.n ?? 0) === 0) throw new HttpError(400, '至少需要保留一名启用的超级管理员');
        }
        sets.push('role = ?');
        binds.push(role);
    }
    if (body.status !== undefined) {
        const status = String(body.status);
        if (status !== 'active' && status !== 'disabled') throw new HttpError(400, 'status 只能是 active 或 disabled');
        if (status === 'disabled' && target.role === 'super' && target.status === 'active') {
            const supers = await env.DB.prepare("SELECT COUNT(*) AS n FROM admin_users WHERE role = 'super' AND status = 'active' AND id <> ?").bind(id).first();
            if ((supers?.n ?? 0) === 0) throw new HttpError(400, '至少需要保留一名启用的超级管理员');
        }
        if (target.id === actor.id && status === 'disabled') throw new HttpError(400, '不能停用自己');
        sets.push('status = ?');
        binds.push(status);
        if (status === 'disabled') {
            await env.DB.prepare("DELETE FROM sessions WHERE principal = 'admin' AND principal_id = ?").bind(id).run();
        }
    }
    if (sets.length === 0) throw new HttpError(400, '没有需要更新的字段');
    await env.DB.prepare(`UPDATE admin_users SET ${sets.join(', ')} WHERE id = ?`).bind(...binds, id).run();
    await audit(env, `admin:${actor.id}`, 'admin.update', target.username, JSON.stringify(body), clientIp(req));
    return json({
        ok: true
    });
}
export async function handleAdminPassword(req, env, id) {
    assertSameOrigin(req);
    const actor = await requireAdmin(env, req, 'super');
    const target = await env.DB.prepare('SELECT * FROM admin_users WHERE id = ?').bind(id).first();
    if (!target) throw new HttpError(404, '管理员不存在');
    const body = await readJson(req);
    const password = String(body.password ?? '');
    if (!PASSWORD_RULE.test(password)) throw new HttpError(400, '密码需 8-64 位，且至少包含一个字母或数字');
    await env.DB.prepare('UPDATE admin_users SET password_hash = ? WHERE id = ?').bind(await hashPassword(password), id).run();
    await env.DB.prepare("DELETE FROM sessions WHERE principal = 'admin' AND principal_id = ?").bind(id).run();
    await audit(env, `admin:${actor.id}`, 'admin.password.reset', target.username, '重置管理员密码', clientIp(req));
    return json({
        ok: true
    });
}
export async function handleAdminDelete(req, env, id) {
    assertSameOrigin(req);
    const actor = await requireAdmin(env, req, 'super');
    if (actor.id === id) throw new HttpError(400, '不能删除自己');
    const target = await env.DB.prepare('SELECT * FROM admin_users WHERE id = ?').bind(id).first();
    if (!target) throw new HttpError(404, '管理员不存在');
    if (target.role === 'super') {
        const supers = await env.DB.prepare("SELECT COUNT(*) AS n FROM admin_users WHERE role = 'super' AND status = 'active' AND id <> ?").bind(id).first();
        if ((supers?.n ?? 0) === 0) throw new HttpError(400, '至少需要保留一名启用的超级管理员');
    }
    await env.DB.batch([
        env.DB.prepare("DELETE FROM sessions WHERE principal = 'admin' AND principal_id = ?").bind(id),
        env.DB.prepare('DELETE FROM admin_users WHERE id = ?').bind(id)
    ]);
    await audit(env, `admin:${actor.id}`, 'admin.delete', target.username, '删除管理员', clientIp(req));
    return json({
        ok: true
    });
}
export async function handleAudit(req, env, url) {
    await requireAdmin(env, req);
    const { page, size, offset } = pageParams(url);
    const keyword = str(url.searchParams.get('q'), 60);
    let where = '1 = 1';
    const binds = [];
    if (keyword) {
        where += ' AND (actor LIKE ? OR action LIKE ? OR target LIKE ? OR detail LIKE ?)';
        const like = likeParam(keyword);
        binds.push(like, like, like, like);
    }
    const total = await env.DB.prepare(`SELECT COUNT(*) AS n FROM audit_log WHERE ${where}`).bind(...binds).first();
    const { results } = await env.DB.prepare(`SELECT * FROM audit_log WHERE ${where} ORDER BY id DESC LIMIT ? OFFSET ?`).bind(...binds, size, offset).all();
    return json({
        total: total?.n ?? 0,
        page,
        size,
        items: results ?? []
    });
}
export async function handleLicenseByKey(req, env, key) {
    await requireAdmin(env, req);
    const license = await env.DB.prepare('SELECT * FROM licenses WHERE license_key = ?').bind(key.toUpperCase()).first();
    if (!license) throw new HttpError(404, '激活码不存在');
    return json({
        key: license.license_key,
        license_id: license.license_id,
        license_type: license.license_type,
        store_id: license.store_id,
        customer_name: license.customer_name,
        issued_at: license.issued_at,
        valid_until: license.valid_until,
        devices: (await env.DB.prepare('SELECT COUNT(*) AS n FROM devices WHERE license_id = ?').bind(license.id).first())?.n ?? 0,
        max_devices: license.max_devices,
        expired: isExpired(license.valid_until),
        revoked: license.status === 'revoked'
    });
}
export async function handleFreeGrant(req, env) {
    assertSameOrigin(req);
    const admin = await requireAdmin(env, req, 'admin');
    const body = await readJson(req);
    const account = str(body.account, 40);
    const user = await env.DB.prepare('SELECT * FROM users WHERE account = ?').bind(account).first();
    if (!user) throw new HttpError(404, '用户不存在');
    const settings = await getSettings(env);
    const granted = await grantFreeLicense(env, user, settings);
    await audit(env, `admin:${admin.id}`, 'license.free', granted.license.license_key, account, clientIp(req));
    return json({
        license: granted.license,
        existed: granted.existed
    });
}