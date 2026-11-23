import { readJson, json, str, clientIp, assertSameOrigin, HttpError, withSetCookies } from '../http.js';
import { getSettings, nowIso, audit } from '../db.js';
import { hashPassword, verifyPassword, createSession, readSession, destroySession, assertLoginAllowed, recordLoginFailure, clearLoginFailures, PASSWORD_RULE } from '../auth.js';
import { grantFreeLicense } from '../services.js';
import { sendMail, codeMail, normalizeEmail, isValidEmail, maskEmail } from '../mail.js';
import { createEmailCode, deleteEmailToken, verifyEmailCode, recordEmailEvent } from '../email-auth.js';
const ACCOUNT_RULE = /^[A-Za-z0-9_.@+-]{4,40}$/;
function publicUser(user) {
    return {
        id: user.id,
        account: user.account,
        store_name: user.store_name,
        customer_name: user.customer_name,
        contact: user.contact,
        email: user.email || '',
        email_verified: Boolean(user.email_verified_at),
        status: user.status,
        created_at: user.created_at,
        last_login_at: user.last_login_at
    };
}
function publicLicense(row) {
    return {
        id: row.id,
        license_key: row.license_key,
        license_id: row.license_id,
        license_type: row.license_type,
        source: row.source,
        store_id: row.store_id,
        valid_until: row.valid_until,
        max_devices: row.max_devices,
        status: row.status,
        note: row.note,
        issued_at: row.issued_at,
        device_count: row.device_count ?? 0
    };
}
export async function requireUser(env, req) {
    const id = await readSession(env, req, 'user');
    if (!id) throw new HttpError(401, '请先登录');
    const user = await env.DB.prepare('SELECT * FROM users WHERE id = ?').bind(id).first();
    if (!user) throw new HttpError(401, '登录状态已失效，请重新登录');
    if (user.status === 'disabled') throw new HttpError(403, '账号已停用，请联系管理员');
    return user;
}
async function userLicenses(env, userId) {
    const { results } = await env.DB.prepare(`SELECT l.*, (SELECT COUNT(*) FROM devices d WHERE d.license_id = l.id) AS device_count
       FROM licenses l WHERE l.user_id = ? ORDER BY l.id DESC LIMIT 50`).bind(userId).all();
    const licenses = (results ?? []).map(publicLicense);
    if (licenses.length === 0) return licenses;
    const { results: devices } = await env.DB.prepare(`SELECT d.id, d.license_id, d.fingerprint, d.first_activated_at, d.last_activated_at, d.activation_count
       FROM devices d JOIN licenses l ON l.id = d.license_id
      WHERE l.user_id = ? ORDER BY d.last_activated_at DESC`).bind(userId).all();
    for (const lic of licenses){
        lic.devices = (devices ?? []).filter((d)=>d.license_id === lic.id);
    }
    return licenses;
}
async function sendCode(req, env, email, purpose, userId) {
    const { token, code } = await createEmailCode(env, req, email, purpose, userId);
    try {
        await sendMail(env, {
            to: email,
            ...codeMail(code, purpose)
        });
    } catch (e) {
        await deleteEmailToken(env, token);
        await recordEmailEvent(env, `code:${purpose}`, email, userId, 'error', String(e).slice(0, 200));
        throw e;
    }
    await recordEmailEvent(env, `code:${purpose}`, email, userId, 'ok', '验证码已发送');
    return json({
        ok: true,
        expires_in: 600
    });
}
export async function handleEmailCode(req, env) {
    assertSameOrigin(req);
    const body = await readJson(req);
    const purpose = str(body.purpose, 20);
    const email = normalizeEmail(body.email);
    if (!isValidEmail(email)) throw new HttpError(400, '请输入正确的邮箱地址');
    if (![
        'register',
        'reset',
        'bind'
    ].includes(purpose)) throw new HttpError(400, '验证码用途不正确');
    if (purpose === 'register') {
        const exists = await env.DB.prepare("SELECT id FROM users WHERE email = ? AND email <> ''").bind(email).first();
        if (exists) throw new HttpError(409, '该邮箱已绑定账号，请直接登录或找回密码');
        return sendCode(req, env, email, purpose, null);
    }
    if (purpose === 'reset') {
        const user = await env.DB.prepare('SELECT * FROM users WHERE email = ? AND email_verified_at IS NOT NULL').bind(email).first();
        if (!user || user.status !== 'active') return json({
            ok: true,
            expires_in: 600
        });
        return sendCode(req, env, email, purpose, user.id);
    }
    const current = await requireUser(env, req);
    const duplicate = await env.DB.prepare("SELECT id FROM users WHERE email = ? AND email <> '' AND id <> ?").bind(email, current.id).first();
    if (duplicate) throw new HttpError(409, '该邮箱已被其他账号绑定');
    if (current.email && current.email_verified_at && current.email !== email) {
        const password = String(body.password ?? '');
        if (!await verifyPassword(password, current.password_hash)) {
            throw new HttpError(400, '更换邮箱需要先验证当前密码');
        }
    }
    return sendCode(req, env, email, purpose, current.id);
}
export async function handleEmailBind(req, env) {
    assertSameOrigin(req);
    const user = await requireUser(env, req);
    const body = await readJson(req);
    const email = normalizeEmail(body.email);
    const code = str(body.email_code, 6);
    if (!isValidEmail(email)) throw new HttpError(400, '请输入正确的邮箱地址');
    const duplicate = await env.DB.prepare("SELECT id FROM users WHERE email = ? AND email <> '' AND id <> ?").bind(email, user.id).first();
    if (duplicate) throw new HttpError(409, '该邮箱已被其他账号绑定');
    await verifyEmailCode(env, email, 'bind', code, user.id);
    try {
        await env.DB.prepare('UPDATE users SET email = ?, email_verified_at = ? WHERE id = ?').bind(email, nowIso(), user.id).run();
    } catch (e) {
        if (String(e).includes('UNIQUE')) throw new HttpError(409, '该邮箱已被其他账号绑定');
        throw new HttpError(500, '邮箱绑定失败，请稍后重试');
    }
    await audit(env, `user:${user.id}`, 'user.email', email, '绑定邮箱', clientIp(req));
    return json({
        ok: true,
        email
    });
}
export async function handleForgotPassword(req, env) {
    assertSameOrigin(req);
    const body = await readJson(req);
    const email = normalizeEmail(body.email);
    if (!isValidEmail(email)) throw new HttpError(400, '请输入正确的邮箱地址');
    const user = await env.DB.prepare('SELECT * FROM users WHERE email = ? AND email_verified_at IS NOT NULL').bind(email).first();
    if (!user || user.status !== 'active') return json({
        ok: true,
        expires_in: 600
    });
    return sendCode(req, env, email, 'reset', user.id);
}
export async function handleResetPassword(req, env) {
    assertSameOrigin(req);
    const body = await readJson(req);
    const email = normalizeEmail(body.email);
    const code = str(body.email_code, 6);
    const newPassword = String(body.new_password ?? '');
    if (!isValidEmail(email)) throw new HttpError(400, '请输入正确的邮箱地址');
    if (!PASSWORD_RULE.test(newPassword)) throw new HttpError(400, '新密码需 8-64 位，且至少包含一个字母或数字');
    const user = await env.DB.prepare('SELECT * FROM users WHERE email = ? AND email_verified_at IS NOT NULL').bind(email).first();
    if (!user || user.status !== 'active') throw new HttpError(400, '邮箱验证码不正确或已过期，请重新获取');
    await verifyEmailCode(env, email, 'reset', code, user.id);
    await env.DB.prepare('UPDATE users SET password_hash = ? WHERE id = ?').bind(await hashPassword(newPassword), user.id).run();
    await env.DB.prepare("DELETE FROM sessions WHERE principal = 'user' AND principal_id = ?").bind(user.id).run();
    await audit(env, `user:${user.id}`, 'user.password.reset', email, '邮箱验证码重置密码', clientIp(req));
    return json({
        ok: true
    });
}
export async function handleLoginCode(req, env) {
    assertSameOrigin(req);
    const body = await readJson(req);
    const account = str(body.account, 40);
    const password = String(body.password ?? '');
    if (!account || !password) throw new HttpError(400, '请先填写账号与密码');
    const ip = clientIp(req);
    await assertLoginAllowed(env, 'user', account, ip);
    const user = await env.DB.prepare('SELECT * FROM users WHERE account = ?').bind(account).first();
    if (!user || !await verifyPassword(password, user.password_hash)) {
        await recordLoginFailure(env, 'user', account, ip);
        throw new HttpError(401, '账号或密码不正确');
    }
    if (user.status === 'disabled') throw new HttpError(403, '账号已停用，请联系管理员');
    if (!user.email || !user.email_verified_at) throw new HttpError(400, '该账号未绑定邮箱，请先用密码登录后在用户中心绑定');
    await clearLoginFailures(env, 'user', account, ip);
    return sendCode(req, env, user.email, 'login', user.id);
}
export async function handleRegister(req, env) {
    assertSameOrigin(req);
    const settings = await getSettings(env);
    if (!settings.registration_open) throw new HttpError(403, '当前未开放注册');
    const body = await readJson(req);
    const account = str(body.account, 40);
    const password = String(body.password ?? '');
    const storeName = str(body.store_name, 40);
    const customerName = str(body.customer_name, 30);
    const contact = str(body.contact, 60);
    const email = normalizeEmail(body.email);
    const emailCode = str(body.email_code, 6);
    if (!ACCOUNT_RULE.test(account)) {
        throw new HttpError(400, '账号需为 4-40 位字母、数字或 _ . @ + -');
    }
    if (!PASSWORD_RULE.test(password)) {
        throw new HttpError(400, '密码需 8-64 位，且至少包含一个字母或数字');
    }
    if (!storeName) throw new HttpError(400, '请填写门店名称');
    if (!customerName) throw new HttpError(400, '请填写联系人');
    if (!isValidEmail(email)) throw new HttpError(400, '请填写正确的邮箱地址');
    const dup = await env.DB.prepare('SELECT id FROM users WHERE account = ?').bind(account).first();
    if (dup) throw new HttpError(409, '该账号已注册，请直接登录');
    const emailDup = await env.DB.prepare("SELECT id FROM users WHERE email = ? AND email <> ''").bind(email).first();
    if (emailDup) throw new HttpError(409, '该邮箱已绑定账号，请直接登录或找回密码');
    await verifyEmailCode(env, email, 'register', emailCode, null);
    let userId = 0;
    try {
        const result = await env.DB.prepare(`INSERT INTO users (account, password_hash, store_name, customer_name, contact, email, email_verified_at, status, note, created_at)
       VALUES (?, ?, ?, ?, ?, ?, ?, 'active', '', ?)`).bind(account, await hashPassword(password), storeName, customerName, contact, email, nowIso(), nowIso()).run();
        userId = Number(result.meta.last_row_id ?? 0);
    } catch (e) {
        if (String(e).includes('UNIQUE')) throw new HttpError(409, '该账号或邮箱已注册，请直接登录');
        throw new HttpError(500, '注册失败，请稍后重试');
    }
    const user = {
        id: userId,
        account,
        password_hash: '',
        store_name: storeName,
        customer_name: customerName,
        contact,
        email,
        email_verified_at: nowIso(),
        status: 'active',
        note: '',
        created_at: nowIso(),
        last_login_at: null
    };
    await audit(env, `user:${userId}`, 'user.register', account, `${storeName} / ${customerName} / ${email}`, clientIp(req));
    let license = null;
    let claimError = '';
    if (settings.free_license_enabled) {
        try {
            const granted = await grantFreeLicense(env, user, settings);
            license = publicLicense(granted.license);
            await audit(env, `user:${userId}`, 'license.free', granted.license.license_key, '注册自动领取', clientIp(req));
        } catch (e) {
            claimError = e instanceof HttpError ? e.message : '自动领取失败，请到用户中心手动领取';
        }
    }
    const session = await createSession(env, req, 'user', userId);
    return withSetCookies(json({
        user: publicUser(user),
        license,
        claim_error: claimError
    }), [
        session.cookie
    ]);
}
export async function handleLogin(req, env) {
    assertSameOrigin(req);
    const body = await readJson(req);
    const account = str(body.account, 40);
    const password = String(body.password ?? '');
    const emailCode = str(body.email_code, 6);
    if (!account || !password) throw new HttpError(400, '请填写账号与密码');
    const ip = clientIp(req);
    await assertLoginAllowed(env, 'user', account, ip);
    const user = await env.DB.prepare('SELECT * FROM users WHERE account = ?').bind(account).first();
    if (!user || !await verifyPassword(password, user.password_hash)) {
        await recordLoginFailure(env, 'user', account, ip);
        throw new HttpError(401, '账号或密码不正确');
    }
    if (user.status === 'disabled') throw new HttpError(403, '账号已停用，请联系管理员');
    const settings = await getSettings(env);
    if (settings.email_login_code && user.email && user.email_verified_at) {
        if (!emailCode) {
            return json({
                error: '请输入邮箱验证码',
                email_code_required: true,
                email: maskEmail(user.email)
            }, 428);
        }
        try {
            await verifyEmailCode(env, user.email, 'login', emailCode, user.id);
        } catch (e) {
            await recordLoginFailure(env, 'user', account, ip);
            throw e;
        }
    }
    await clearLoginFailures(env, 'user', account, ip);
    await env.DB.prepare('UPDATE users SET last_login_at = ? WHERE id = ?').bind(nowIso(), user.id).run();
    const session = await createSession(env, req, 'user', user.id);
    return withSetCookies(json({
        user: publicUser(user)
    }), [
        session.cookie
    ]);
}
export async function handleLogout(req, env) {
    assertSameOrigin(req);
    return withSetCookies(json({
        ok: true
    }), [
        await destroySession(env, req, 'user')
    ]);
}
export async function handleMe(req, env) {
    const user = await requireUser(env, req);
    const settings = await getSettings(env);
    return json({
        user: publicUser(user),
        settings: {
            free_license_enabled: settings.free_license_enabled,
            free_license_days: settings.free_license_days,
            free_max_devices: settings.free_max_devices,
            site_notice: settings.site_notice,
            site_name: settings.site_name,
            site_keywords: settings.site_keywords,
            site_description: settings.site_description,
            contact_email: settings.contact_email,
            contact_phone: settings.contact_phone,
            contact_address: settings.contact_address,
            icp_no: settings.icp_no,
            email_login_code: settings.email_login_code
        }
    });
}
export async function handleClaim(req, env) {
    assertSameOrigin(req);
    const user = await requireUser(env, req);
    const settings = await getSettings(env);
    const granted = await grantFreeLicense(env, user, settings);
    await audit(env, `user:${user.id}`, 'license.free', granted.license.license_key, granted.existed ? '已存在有效免费授权' : '用户中心领取', clientIp(req));
    return json({
        license: publicLicense(granted.license),
        existed: granted.existed
    });
}
export async function handleLicenses(req, env) {
    const user = await requireUser(env, req);
    return json({
        licenses: await userLicenses(env, user.id)
    });
}
export async function handleChangePassword(req, env) {
    assertSameOrigin(req);
    const user = await requireUser(env, req);
    const body = await readJson(req);
    const oldPassword = String(body.old_password ?? '');
    const newPassword = String(body.new_password ?? '');
    if (!PASSWORD_RULE.test(newPassword)) {
        throw new HttpError(400, '新密码需 8-64 位，且至少包含一个字母或数字');
    }
    if (!await verifyPassword(oldPassword, user.password_hash)) {
        throw new HttpError(400, '原密码不正确');
    }
    await env.DB.prepare('UPDATE users SET password_hash = ? WHERE id = ?').bind(await hashPassword(newPassword), user.id).run();
    await audit(env, `user:${user.id}`, 'user.password', user.account, '用户修改密码', clientIp(req));
    return json({
        ok: true
    });
}
export async function handlePublicConfig(env) {
    const settings = await getSettings(env);
    return json({
        registration_open: settings.registration_open,
        free_license_enabled: settings.free_license_enabled,
        free_license_days: settings.free_license_days,
        free_max_devices: settings.free_max_devices,
        site_notice: settings.site_notice,
        site_name: settings.site_name,
        site_keywords: settings.site_keywords,
        site_description: settings.site_description,
        contact_email: settings.contact_email,
        contact_phone: settings.contact_phone,
        contact_address: settings.contact_address,
        icp_no: settings.icp_no,
        email_login_code: settings.email_login_code
    });
}
export async function handleUnbindRequest(req, env) {
    assertSameOrigin(req);
    const user = await requireUser(env, req);
    const body = await readJson(req);
    const licenseId = Number(body.license_id);
    const deviceId = Number(body.device_id);
    const reason = str(body.reason, 200);
    if (!Number.isInteger(licenseId) || !Number.isInteger(deviceId)) throw new HttpError(400, '设备信息不完整');
    const device = await env.DB.prepare(`SELECT d.id, d.license_id FROM devices d
       JOIN licenses l ON l.id = d.license_id
      WHERE d.id = ? AND d.license_id = ? AND l.user_id = ?`).bind(deviceId, licenseId, user.id).first();
    if (!device) throw new HttpError(404, '设备不存在或不属于当前账号');
    const pending = await env.DB.prepare(`SELECT id FROM unbind_requests WHERE device_id = ? AND status = 'pending'`).bind(deviceId).first();
    if (pending) throw new HttpError(409, '该设备已有待处理申请');
    await env.DB.prepare('INSERT INTO unbind_requests (user_id, license_id, device_id, reason, created_at) VALUES (?, ?, ?, ?, ?)').bind(user.id, licenseId, deviceId, reason, nowIso()).run();
    await audit(env, `user:${user.id}`, 'unbind.request', String(deviceId), reason || '申请解绑设备', clientIp(req));
    return json({
        ok: true,
        message: '解绑申请已提交，管理员审核后生效'
    });
}