import { readJson, json, str, clientIp, assertSameOrigin, HttpError, clampInt, withSetCookies } from '../http.js';
import { nowIso, audit } from '../db.js';
import { hashPassword, verifyPassword, sessionHash } from '../auth.js';
import { verifyTotp, generateTotpSecret, otpauthUrl, currentTotp } from '../totp.js';
import { requireAdmin } from './admin.js';
const EXPORT_TABLES = {
    users: 'users',
    licenses: 'licenses',
    devices: 'devices',
    activations: 'activations',
    audit: 'audit_log',
    feedback: 'feedback',
    unbind: 'unbind_requests',
    releases: 'releases',
    docs: 'docs'
};
function clean(s, max) {
    return str(s, max);
}
function csvCell(v) {
    const s = String(v ?? '');
    return /[",\n\r]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
}
function csvResponse(rows, name) {
    const headers = rows.length ? Object.keys(rows[0]) : [];
    const body = '\ufeff' + [
        headers.join(','),
        ...rows.map((r)=>headers.map((h)=>csvCell(r[h])).join(','))
    ].join('\r\n');
    return new Response(body, {
        headers: {
            'Content-Type': 'text/csv; charset=utf-8',
            'Content-Disposition': `attachment; filename="${name}-${new Date().toISOString().slice(0, 10)}.csv"`,
            'Cache-Control': 'no-store'
        }
    });
}
export async function handleDevices(req, env, url) {
    await requireAdmin(env, req);
    const page = clampInt(url.searchParams.get('page'), 1, 1, 10_000);
    const size = clampInt(url.searchParams.get('size'), 25, 1, 100);
    const offset = (page - 1) * size;
    const q = clean(url.searchParams.get('q'), 60);
    const active = clean(url.searchParams.get('active'), 20);
    const where = [];
    const binds = [];
    if (q) {
        where.push('(d.fingerprint LIKE ? OR d.last_ip LIKE ? OR l.license_key LIKE ? OR u.account LIKE ? OR u.store_name LIKE ?)');
        binds.push(...Array(5).fill(`%${q.replace(/%/g, '')}%`));
    }
    if (active === '24h' || active === '7d' || active === '30d') {
        const ms = active === '24h' ? 86_400_000 : active === '7d' ? 7 * 86_400_000 : 30 * 86_400_000;
        where.push('d.last_activated_at >= ?');
        binds.push(new Date(Date.now() - ms).toISOString());
    }
    const clause = where.length ? `WHERE ${where.join(' AND ')}` : '';
    const now = Date.now();
    const d24 = new Date(now - 86_400_000).toISOString();
    const d7 = new Date(now - 7 * 86_400_000).toISOString();
    const d30 = new Date(now - 30 * 86_400_000).toISOString();
    const [total, rows, count24, count7, count30, regions] = await Promise.all([
        env.DB.prepare(`SELECT COUNT(*) AS n FROM devices d LEFT JOIN licenses l ON l.id=d.license_id LEFT JOIN users u ON u.id=l.user_id ${clause}`).bind(...binds).first(),
        env.DB.prepare(`SELECT d.id, d.fingerprint, d.first_activated_at, d.last_activated_at, d.activation_count,
              d.last_ip, d.country, d.region, d.city, l.license_key, l.id AS license_key_id,
              l.store_id, u.account, u.store_name, u.customer_name
         FROM devices d LEFT JOIN licenses l ON l.id=d.license_id LEFT JOIN users u ON u.id=l.user_id
         ${clause} ORDER BY d.last_activated_at DESC LIMIT ? OFFSET ?`).bind(...binds, size, offset).all(),
        env.DB.prepare('SELECT COUNT(*) AS n FROM devices WHERE last_activated_at >= ?').bind(d24).first(),
        env.DB.prepare('SELECT COUNT(*) AS n FROM devices WHERE last_activated_at >= ?').bind(d7).first(),
        env.DB.prepare('SELECT COUNT(*) AS n FROM devices WHERE last_activated_at >= ?').bind(d30).first(),
        env.DB.prepare(`SELECT CASE WHEN country = '' THEN '未记录' ELSE country END AS name, COUNT(*) AS n
         FROM devices GROUP BY name ORDER BY n DESC LIMIT 12`).all()
    ]);
    return json({
        total: total?.n ?? 0,
        page,
        size,
        items: rows.results ?? [],
        summary: {
            active24: count24?.n ?? 0,
            active7: count7?.n ?? 0,
            active30: count30?.n ?? 0
        },
        regions: regions.results ?? []
    });
}
export async function handleReleases(req, env, url) {
    if (req.method === 'GET') {
        await requireAdmin(env, req);
        const { results } = await env.DB.prepare('SELECT * FROM releases ORDER BY is_current DESC, published_at DESC, id DESC').all();
        return json({
            releases: results ?? []
        });
    }
    await requireAdmin(env, req, 'admin');
    assertSameOrigin(req);
    const body = await readJson(req);
    const version = clean(body.version, 40);
    const fileUrl = clean(body.file_url, 500);
    if (!version || !fileUrl) throw new HttpError(400, '版本号和下载地址不能为空');
    const isCurrent = body.is_current ? 1 : 0;
    if (isCurrent) await env.DB.prepare('UPDATE releases SET is_current = 0').run();
    const now = nowIso();
    const res = await env.DB.prepare(`INSERT INTO releases (version, platform, channel, file_url, file_size, sha256, notes, is_current, published_at, created_at)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`).bind(version, clean(body.platform, 20) || 'Windows', clean(body.channel, 20) || 'stable', fileUrl, Number(body.file_size) || 0, clean(body.sha256, 128), clean(body.notes, 2000), isCurrent, clean(body.published_at, 40) || now, now).run();
    await audit(env, `admin:${(await requireAdmin(env, req)).id}`, 'release.create', version, `发布客户端 ${version}`, clientIp(req));
    return json({
        ok: true,
        id: res.meta.last_row_id
    });
}
export async function handleReleaseUpdate(req, env, id) {
    const admin = await requireAdmin(env, req, 'admin');
    assertSameOrigin(req);
    const body = await readJson(req);
    const fields = [];
    const binds = [];
    const text = (key, max)=>{
        if (body[key] === undefined) return;
        fields.push(`${key} = ?`);
        binds.push(clean(body[key], max));
    };
    text('version', 40);
    text('platform', 20);
    text('channel', 20);
    text('file_url', 500);
    text('sha256', 128);
    text('notes', 2000);
    text('published_at', 40);
    if (body.file_size !== undefined) {
        fields.push('file_size = ?');
        binds.push(Number(body.file_size) || 0);
    }
    if (body.is_current !== undefined) {
        if (body.is_current) await env.DB.prepare('UPDATE releases SET is_current = 0').run();
        fields.push('is_current = ?');
        binds.push(body.is_current ? 1 : 0);
    }
    if (!fields.length) throw new HttpError(400, '没有需要更新的字段');
    await env.DB.prepare(`UPDATE releases SET ${fields.join(', ')} WHERE id = ?`).bind(...binds, id).run();
    await audit(env, `admin:${admin.id}`, 'release.update', String(id), JSON.stringify(body), clientIp(req));
    return json({
        ok: true
    });
}
export async function handleReleaseDelete(req, env, id) {
    const admin = await requireAdmin(env, req, 'admin');
    assertSameOrigin(req);
    await env.DB.prepare('DELETE FROM releases WHERE id = ?').bind(id).run();
    await audit(env, `admin:${admin.id}`, 'release.delete', String(id), '删除客户端版本', clientIp(req));
    return json({
        ok: true
    });
}
export async function handlePublicReleases(env) {
    const { results } = await env.DB.prepare(`SELECT id, version, platform, channel, file_url, file_size, sha256, notes, is_current, downloads, published_at
       FROM releases ORDER BY is_current DESC, published_at DESC, id DESC`).all();
    return json({
        releases: results ?? []
    });
}
export async function handleReleaseDownload(req, env, id) {
    const row = await env.DB.prepare('SELECT file_url FROM releases WHERE id = ?').bind(id).first();
    if (!row) throw new HttpError(404, '版本不存在');
    await env.DB.prepare('UPDATE releases SET downloads = downloads + 1 WHERE id = ?').bind(id).run();
    return Response.redirect(new URL(row.file_url, req.url).toString(), 302);
}
export async function handleDocs(req, env) {
    if (req.method === 'GET') {
        const admin = await requireAdmin(env, req).catch(()=>null);
        const { results } = await env.DB.prepare(admin ? 'SELECT * FROM docs ORDER BY category, sort, id' : 'SELECT * FROM docs WHERE published = 1 ORDER BY category, sort, id').all();
        return json({
            docs: results ?? []
        });
    }
    const admin = await requireAdmin(env, req, 'admin');
    assertSameOrigin(req);
    const body = await readJson(req);
    const question = clean(body.question, 200);
    const answer = clean(body.answer, 4000);
    if (!question || !answer) throw new HttpError(400, '问题和答案不能为空');
    const now = nowIso();
    const res = await env.DB.prepare(`INSERT INTO docs (category, question, answer, sort, published, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)`).bind(clean(body.category, 40) || '快速开始', question, answer, Number(body.sort) || 0, body.published === false ? 0 : 1, now, now).run();
    await audit(env, `admin:${admin.id}`, 'doc.create', question.slice(0, 60), '新增帮助内容', clientIp(req));
    return json({
        ok: true,
        id: res.meta.last_row_id
    });
}
export async function handleDocUpdate(req, env, id) {
    const admin = await requireAdmin(env, req, 'admin');
    assertSameOrigin(req);
    const body = await readJson(req);
    const fields = [];
    const binds = [];
    if (body.category !== undefined) {
        fields.push('category = ?');
        binds.push(clean(body.category, 40));
    }
    if (body.question !== undefined) {
        fields.push('question = ?');
        binds.push(clean(body.question, 200));
    }
    if (body.answer !== undefined) {
        fields.push('answer = ?');
        binds.push(clean(body.answer, 4000));
    }
    if (body.sort !== undefined) {
        fields.push('sort = ?');
        binds.push(Number(body.sort) || 0);
    }
    if (body.published !== undefined) {
        fields.push('published = ?');
        binds.push(body.published ? 1 : 0);
    }
    if (!fields.length) throw new HttpError(400, '没有需要更新的字段');
    fields.push('updated_at = ?');
    binds.push(nowIso());
    await env.DB.prepare(`UPDATE docs SET ${fields.join(', ')} WHERE id = ?`).bind(...binds, id).run();
    await audit(env, `admin:${admin.id}`, 'doc.update', String(id), JSON.stringify(body), clientIp(req));
    return json({
        ok: true
    });
}
export async function handleDocDelete(req, env, id) {
    const admin = await requireAdmin(env, req, 'admin');
    assertSameOrigin(req);
    await env.DB.prepare('DELETE FROM docs WHERE id = ?').bind(id).run();
    await audit(env, `admin:${admin.id}`, 'doc.delete', String(id), '删除帮助内容', clientIp(req));
    return json({
        ok: true
    });
}
export async function handlePublicDocs(env) {
    const { results } = await env.DB.prepare('SELECT id, category, question, answer, sort FROM docs WHERE published = 1 ORDER BY category, sort, id').all();
    return json({
        docs: results ?? []
    });
}
export async function handlePublicFeedback(req, env) {
    assertSameOrigin(req);
    const body = await readJson(req);
    const content = clean(body.content, 1000);
    if (!content) throw new HttpError(400, '请填写反馈内容');
    await env.DB.prepare('INSERT INTO feedback (name, contact, content, ip, created_at) VALUES (?, ?, ?, ?, ?)').bind(clean(body.name, 40), clean(body.contact, 80), content, clientIp(req).slice(0, 60), nowIso()).run();
    return json({
        ok: true,
        message: '反馈已提交，我们会尽快处理'
    });
}
export async function handleFeedback(req, env, url) {
    await requireAdmin(env, req);
    const status = clean(url.searchParams.get('status'), 20);
    const where = status === 'open' || status === 'closed' ? 'WHERE status = ?' : '';
    const { results } = await env.DB.prepare(`SELECT * FROM feedback ${where} ORDER BY id DESC LIMIT 200`).bind(...where ? [
        status
    ] : []).all();
    return json({
        items: results ?? []
    });
}
export async function handleFeedbackUpdate(req, env, id) {
    const admin = await requireAdmin(env, req, 'admin');
    assertSameOrigin(req);
    const body = await readJson(req);
    const status = clean(body.status, 20);
    if (status !== 'open' && status !== 'closed') throw new HttpError(400, '状态不合法');
    await env.DB.prepare('UPDATE feedback SET status = ?, handled_at = ? WHERE id = ?').bind(status, nowIso(), id).run();
    await audit(env, `admin:${admin.id}`, 'feedback.update', String(id), status, clientIp(req));
    return json({
        ok: true
    });
}
export async function handleUnbindRequests(req, env) {
    await requireAdmin(env, req);
    const { results } = await env.DB.prepare(`SELECT r.*, u.account, u.store_name, l.license_key, d.fingerprint, d.last_ip, d.country, d.city
       FROM unbind_requests r
       LEFT JOIN users u ON u.id=r.user_id
       LEFT JOIN licenses l ON l.id=r.license_id
       LEFT JOIN devices d ON d.id=r.device_id
      ORDER BY CASE r.status WHEN 'pending' THEN 0 ELSE 1 END, r.id DESC LIMIT 200`).all();
    return json({
        items: results ?? []
    });
}
export async function handleUnbindRequestUpdate(req, env, id) {
    const admin = await requireAdmin(env, req, 'admin');
    assertSameOrigin(req);
    const body = await readJson(req);
    const status = clean(body.status, 20);
    if (status !== 'approved' && status !== 'rejected') throw new HttpError(400, '状态不合法');
    const row = await env.DB.prepare('SELECT * FROM unbind_requests WHERE id = ?').bind(id).first();
    if (!row) throw new HttpError(404, '申请不存在');
    if (row.status !== 'pending') throw new HttpError(409, '该申请已处理');
    if (status === 'approved') {
        await env.DB.prepare('DELETE FROM devices WHERE id = ? AND license_id = ?').bind(row.device_id, row.license_id).run();
    }
    await env.DB.prepare('UPDATE unbind_requests SET status = ?, handled_at = ?, handled_by = ? WHERE id = ?').bind(status, nowIso(), admin.id, id).run();
    await audit(env, `admin:${admin.id}`, `unbind.${status}`, String(id), `解绑设备申请 ${status}`, clientIp(req));
    return json({
        ok: true
    });
}
export async function handleNotifications(req, env, url) {
    const admin = await requireAdmin(env, req);
    const sinceHour = new Date(Date.now() - 3600_000).toISOString();
    const soon = new Date(Date.now() + 30 * 86_400_000).toISOString();
    const now = nowIso();
    const [expiring, failures, fingerprints, requests, feedbackRows, reads] = await Promise.all([
        env.DB.prepare(`SELECT id, license_key, store_id, valid_until FROM licenses
        WHERE status='active' AND valid_until IS NOT NULL AND valid_until > ? AND valid_until <= ?
        ORDER BY valid_until LIMIT 20`).bind(now, soon).all(),
        env.DB.prepare(`SELECT COUNT(*) AS n FROM activations WHERE result='error' AND created_at >= ?`).bind(sinceHour).first(),
        env.DB.prepare(`SELECT fingerprint, COUNT(DISTINCT license_id) AS n FROM devices
        GROUP BY fingerprint HAVING n > 1 ORDER BY n DESC LIMIT 10`).all(),
        env.DB.prepare(`SELECT id, license_id, reason, created_at FROM unbind_requests WHERE status='pending' ORDER BY id DESC LIMIT 20`).all(),
        env.DB.prepare(`SELECT id, content, created_at FROM feedback WHERE status='open' ORDER BY id DESC LIMIT 20`).all(),
        env.DB.prepare('SELECT alert_key FROM alert_reads WHERE admin_id = ?').bind(admin.id).all()
    ]);
    const readSet = new Set((reads.results ?? []).map((r)=>r.alert_key));
    const alerts = [];
    const push = (key, severity, title, body, link, created)=>{
        alerts.push({
            key,
            severity,
            title,
            body,
            link,
            created_at: created || now,
            read: readSet.has(key)
        });
    };
    for (const l of expiring.results ?? []){
        push(`license-exp-${l.id}-${String(l.valid_until).slice(0, 10)}`, 'warning', '授权即将到期', `${l.license_key} · ${l.store_id || '未填写门店'} · ${String(l.valid_until).slice(0, 10)}`, `/admin/licenses?q=${encodeURIComponent(String(l.license_key))}`);
    }
    if ((failures?.n ?? 0) > 5) {
        const hour = now.slice(0, 13);
        push(`activation-fail-${hour}`, 'danger', '激活失败量异常', `最近 1 小时有 ${failures?.n ?? 0} 次失败激活，请检查授权与客户端配置。`, '/admin/activations?result=error');
    }
    for (const f of fingerprints.results ?? []){
        push(`fingerprint-${f.fingerprint}`, 'danger', '同一设备指纹绑定多个授权', `设备 ${String(f.fingerprint).slice(0, 16)}… 已出现在 ${f.n} 个授权下，可能存在共享或异常换机。`, '/admin/devices');
    }
    for (const r of requests.results ?? []){
        push(`request-${r.id}`, 'warning', '有新的解绑申请', `申请 #${r.id} 等待处理：${r.reason || '未填写原因'}`, '/admin/requests');
    }
    for (const f of feedbackRows.results ?? []){
        push(`feedback-${f.id}`, 'info', '收到新的用户反馈', `反馈 #${f.id}：${String(f.content).slice(0, 60)}`, '/admin/feedback');
    }
    alerts.sort((a, b)=>String(b.created_at).localeCompare(String(a.created_at)));
    return json({
        items: alerts.slice(0, 100),
        unread: alerts.filter((a)=>!a.read).length
    });
}
export async function handleNotificationRead(req, env) {
    const admin = await requireAdmin(env, req);
    assertSameOrigin(req);
    const body = await readJson(req);
    const keys = Array.isArray(body.keys) ? body.keys.slice(0, 100).map((k)=>clean(k, 200)) : [];
    if (!keys.length) throw new HttpError(400, '缺少通知标识');
    const stmts = keys.map((key)=>env.DB.prepare('INSERT INTO alert_reads (admin_id, alert_key, read_at) VALUES (?, ?, ?) ON CONFLICT(admin_id, alert_key) DO UPDATE SET read_at=excluded.read_at').bind(admin.id, key, nowIso()));
    await env.DB.batch(stmts);
    return json({
        ok: true
    });
}
export async function handleSecurity(req, env, url) {
    await requireAdmin(env, req);
    const part = url.searchParams.get('part');
    if (part === 'blacklist') {
        const blacklist = await env.DB.prepare('SELECT * FROM blacklist ORDER BY id DESC LIMIT 200').all();
        return json({
            blacklist: blacklist.results ?? []
        });
    }
    const hourAgo = new Date(Date.now() - 3600_000).toISOString();
    const dayAgo = new Date(Date.now() - 86_400_000).toISOString();
    const [ipAnomaly, fpAnomaly, rate] = await Promise.all([
        env.DB.prepare(`SELECT ip, COUNT(DISTINCT license_id) AS licenses, COUNT(*) AS hits FROM activations
        WHERE created_at >= ? AND ip <> '' GROUP BY ip HAVING licenses > 3 ORDER BY licenses DESC LIMIT 10`).bind(dayAgo).all(),
        env.DB.prepare(`SELECT fingerprint, COUNT(DISTINCT license_id) AS licenses FROM devices
        GROUP BY fingerprint HAVING licenses > 1 ORDER BY licenses DESC LIMIT 10`).all(),
        env.DB.prepare('SELECT ip, COUNT(*) AS n FROM activations WHERE created_at >= ? GROUP BY ip ORDER BY n DESC LIMIT 10').bind(hourAgo).all()
    ]);
    return json({
        anomalies: {
            ips: ipAnomaly.results ?? [],
            fingerprints: fpAnomaly.results ?? []
        },
        rate: rate.results ?? [],
        limit_per_hour: 120
    });
}
export async function handleBlacklistAdd(req, env) {
    const admin = await requireAdmin(env, req, 'admin');
    assertSameOrigin(req);
    const body = await readJson(req);
    const kind = clean(body.kind, 20);
    const value = clean(body.value, 160);
    if (![
        'ip',
        'fingerprint',
        'license'
    ].includes(kind)) throw new HttpError(400, '类型不合法');
    if (!value) throw new HttpError(400, '请输入要拦截的值');
    await env.DB.prepare('INSERT OR IGNORE INTO blacklist (kind, value, reason, created_at) VALUES (?, ?, ?, ?)').bind(kind, value, clean(body.reason, 200), nowIso()).run();
    await audit(env, `admin:${admin.id}`, 'blacklist.add', `${kind}:${value}`, clean(body.reason, 200), clientIp(req));
    return json({
        ok: true
    });
}
export async function handleBlacklistDelete(req, env, id) {
    const admin = await requireAdmin(env, req, 'admin');
    assertSameOrigin(req);
    await env.DB.prepare('DELETE FROM blacklist WHERE id = ?').bind(id).run();
    await audit(env, `admin:${admin.id}`, 'blacklist.delete', String(id), '移除黑名单', clientIp(req));
    return json({
        ok: true
    });
}
export async function handleAccountSecurity(req, env, url) {
    const admin = await requireAdmin(env, req);
    const part = url.searchParams.get('part');
    const totpEnabled = Boolean(Number(admin.totp_enabled || 0));
    if (part === 'totp') return json({
        totp_enabled: totpEnabled
    });
    if (part === 'sessions') {
        const current = await sessionHash(req, 'admin');
        const { results } = await env.DB.prepare(`SELECT token AS id, created_at, expires_at, ip, user_agent FROM sessions
        WHERE principal='admin' AND principal_id=? ORDER BY created_at DESC`).bind(admin.id).all();
        return json({
            sessions: (results ?? []).map((s)=>({
                    ...s,
                    current: s.id === current
                }))
        });
    }
    const current = await sessionHash(req, 'admin');
    const { results } = await env.DB.prepare(`SELECT token AS id, created_at, expires_at, ip, user_agent FROM sessions
      WHERE principal='admin' AND principal_id=? ORDER BY created_at DESC`).bind(admin.id).all();
    return json({
        totp_enabled: totpEnabled,
        sessions: (results ?? []).map((s)=>({
                ...s,
                current: s.id === current
            }))
    });
}
export async function handleTotpSetup(req, env) {
    const admin = await requireAdmin(env, req);
    assertSameOrigin(req);
    if (admin.totp_enabled) throw new HttpError(409, '已开启动态验证码');
    const secret = generateTotpSecret();
    await env.DB.prepare('UPDATE admin_users SET totp_secret = ?, totp_enabled = 0 WHERE id = ?').bind(secret, admin.id).run();
    return json({
        secret,
        otpauth: otpauthUrl(secret, admin.username)
    });
}
export async function handleTotpEnable(req, env) {
    const admin = await requireAdmin(env, req);
    assertSameOrigin(req);
    const body = await readJson(req);
    if (!admin.totp_secret) throw new HttpError(400, '请先生成密钥');
    if (!await verifyTotp(admin.totp_secret, clean(body.code, 8))) throw new HttpError(400, '验证码不正确');
    await env.DB.prepare('UPDATE admin_users SET totp_enabled = 1 WHERE id = ?').bind(admin.id).run();
    await audit(env, `admin:${admin.id}`, 'security.totp.enable', admin.username, '开启动态验证码', clientIp(req));
    return json({
        ok: true
    });
}
export async function handleTotpDisable(req, env) {
    const admin = await requireAdmin(env, req);
    assertSameOrigin(req);
    const body = await readJson(req);
    if (!await verifyPassword(String(body.password ?? ''), admin.password_hash)) throw new HttpError(400, '密码不正确');
    if (!await verifyTotp(admin.totp_secret, clean(body.code, 8))) throw new HttpError(400, '验证码不正确');
    await env.DB.prepare('UPDATE admin_users SET totp_enabled = 0, totp_secret = ? WHERE id = ?').bind('', admin.id).run();
    await audit(env, `admin:${admin.id}`, 'security.totp.disable', admin.username, '关闭动态验证码', clientIp(req));
    return json({
        ok: true
    });
}
export async function handleSessionDelete(req, env, id) {
    const admin = await requireAdmin(env, req);
    assertSameOrigin(req);
    if (id === 'others') {
        const current = await sessionHash(req, 'admin');
        await env.DB.prepare("DELETE FROM sessions WHERE principal='admin' AND principal_id=? AND token<>?").bind(admin.id, current).run();
    } else {
        await env.DB.prepare("DELETE FROM sessions WHERE principal='admin' AND principal_id=? AND token=?").bind(admin.id, id).run();
    }
    return json({
        ok: true
    });
}
export async function handleCurrentPassword(req, env) {
    const admin = await requireAdmin(env, req);
    assertSameOrigin(req);
    const body = await readJson(req);
    const current = String(body.current_password ?? '');
    const next = String(body.new_password ?? '');
    if (!await verifyPassword(current, admin.password_hash)) throw new HttpError(400, '当前密码不正确');
    if (!PASSWORD_RULE.test(next)) throw new HttpError(400, '新密码至少 8 位，需包含字母和数字');
    await env.DB.prepare('UPDATE admin_users SET password_hash = ? WHERE id = ?').bind(await hashPassword(next), admin.id).run();
    await audit(env, `admin:${admin.id}`, 'admin.password.self', admin.username, '修改本人密码', clientIp(req));
    return json({
        ok: true
    });
}
const PASSWORD_RULE = /^(?=.*[A-Za-z])(?=.*\d).{8,64}$/;
export async function handleExport(req, env, url) {
    await requireAdmin(env, req, 'super');
    const type = clean(url.searchParams.get('type'), 20);
    const table = EXPORT_TABLES[type];
    if (!table) throw new HttpError(400, '不支持的导出类型');
    const { results } = await env.DB.prepare(`SELECT * FROM ${table} LIMIT 50000`).all();
    return csvResponse(results ?? [], `checkout-r-${type}`);
}
export async function handleBackup(req, env) {
    const admin = await requireAdmin(env, req, 'super');
    assertSameOrigin(req);
    const tables = [
        'users',
        'licenses',
        'devices',
        'activations',
        'admin_users',
        'settings',
        'audit_log',
        'releases',
        'docs',
        'feedback',
        'unbind_requests',
        'blacklist',
        'page_views'
    ];
    const data = {};
    for (const table of tables){
        const { results } = await env.DB.prepare(`SELECT * FROM ${table} LIMIT 50000`).all();
        data[table] = results ?? [];
    }
    await audit(env, `admin:${admin.id}`, 'backup.download', 'all', '导出全量数据备份', clientIp(req));
    return new Response(JSON.stringify({
        exported_at: nowIso(),
        tables: data
    }, null, 2), {
        headers: {
            'Content-Type': 'application/json; charset=utf-8',
            'Content-Disposition': `attachment; filename="checkout-r-backup-${new Date().toISOString().slice(0, 10)}.json"`,
            'Cache-Control': 'no-store'
        }
    });
}
export async function handleBackupInfo(req, env) {
    await requireAdmin(env, req, 'super');
    const counts = {};
    for (const table of Object.values(EXPORT_TABLES).filter((v, i, a)=>a.indexOf(v) === i)){
        const row = await env.DB.prepare(`SELECT COUNT(*) AS n FROM ${table}`).first();
        counts[table] = row?.n ?? 0;
    }
    return json({
        counts,
        tables: EXPORT_TABLES
    });
}
export async function handleSearch(req, env, url) {
    await requireAdmin(env, req);
    const q = clean(url.searchParams.get('q'), 60);
    if (q.length < 2) return json({
        items: []
    });
    const like = `%${q}%`;
    const [users, licenses, devices] = await Promise.all([
        env.DB.prepare(`SELECT id, store_name, account, phone FROM users
        WHERE store_name LIKE ? OR account LIKE ? OR phone LIKE ? ORDER BY id DESC LIMIT 8`).bind(like, like, like).all(),
        env.DB.prepare(`SELECT id, license_key, status, store_id FROM licenses
        WHERE license_key LIKE ? ORDER BY id DESC LIMIT 8`).bind(like).all(),
        env.DB.prepare(`SELECT id, fingerprint, last_ip, store_id FROM devices
        WHERE fingerprint LIKE ? OR last_ip LIKE ? ORDER BY id DESC LIMIT 8`).bind(like, like).all()
    ]);
    const items = [];
    for (const u of users.results ?? []){
        items.push({
            type: 'user',
            label: String(u.store_name || u.account || u.phone || '用户'),
            sub: String(u.account || u.phone || ''),
            href: `/admin/users?q=${encodeURIComponent(q)}`
        });
    }
    for (const l of licenses.results ?? []){
        items.push({
            type: 'license',
            label: String(l.license_key || ''),
            sub: `状态 ${l.status || ''}`,
            href: `/admin/licenses?q=${encodeURIComponent(String(l.license_key || ''))}`
        });
    }
    for (const d of devices.results ?? []){
        items.push({
            type: 'device',
            label: String(d.fingerprint || ''),
            sub: `IP ${d.last_ip || '—'}`,
            href: `/admin/devices?q=${encodeURIComponent(String(d.fingerprint || ''))}`
        });
    }
    return json({
        items
    });
}