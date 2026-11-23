import { readJson, corsJson, str, clientIp, userAgent, HttpError } from '../http.js';
import { getSettings, nowIso } from '../db.js';
import { signLicense, buildPayload, withinGrace, PRODUCT } from '../license.js';
function geo(req) {
    const cf = req.cf || {};
    const clean = (v)=>String(v ?? '').slice(0, 80);
    return {
        country: clean(cf.country),
        region: clean(cf.region),
        city: clean(cf.city)
    };
}
async function logActivation(env, req, license, licenseKey, storeId, fingerprint, result, reason) {
    const g = geo(req);
    await env.DB.prepare(`INSERT INTO activations
      (license_id, license_key, user_id, store_id, device_fingerprint, ip, user_agent, result, reason, created_at, country, region, city)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`).bind(license?.id ?? null, licenseKey.slice(0, 64), license?.user_id ?? null, storeId.slice(0, 60), fingerprint.slice(0, 128), clientIp(req).slice(0, 60), userAgent(req), result, reason.slice(0, 200), nowIso(), g.country, g.region, g.city).run();
}
export async function handleActivate(req, env) {
    const body = await readJson(req);
    const key = str(body.activation_key, 64).toUpperCase();
    if (!key) throw new HttpError(400, '请输入激活码');
    const fingerprint = str(body.device_fingerprint, 128);
    const storeId = str(body.store_id, 60);
    const ip = clientIp(req).slice(0, 60);
    const blocked = await env.DB.prepare(`SELECT reason FROM blacklist
      WHERE (kind = 'ip' AND value = ?) OR (kind = 'fingerprint' AND value = ?)
      LIMIT 1`).bind(ip, fingerprint).first();
    if (blocked) {
        await logActivation(env, req, null, key, storeId, fingerprint, 'error', `风控拦截：${blocked.reason || '黑名单'}`);
        throw new HttpError(403, '该设备或网络已被限制激活');
    }
    const hourAgo = new Date(Date.now() - 3600_000).toISOString();
    const rate = await env.DB.prepare(`SELECT COUNT(*) AS n FROM activations WHERE ip = ? AND created_at >= ?`).bind(ip, hourAgo).first();
    if ((rate?.n ?? 0) > 120) {
        await logActivation(env, req, null, key, storeId, fingerprint, 'error', '激活请求过于频繁');
        throw new HttpError(429, '激活请求过于频繁，请稍后再试');
    }
    const license = await env.DB.prepare('SELECT * FROM licenses WHERE license_key = ?').bind(key).first();
    if (!license) {
        await logActivation(env, req, null, key, storeId, fingerprint, 'error', '激活码不存在');
        throw new HttpError(404, '激活码不存在');
    }
    const settings = await getSettings(env);
    if (license.status === 'revoked') {
        await logActivation(env, req, license, key, storeId, fingerprint, 'error', '授权已停用');
        throw new HttpError(403, '该激活码已停用');
    }
    if (withinGrace(license.valid_until, settings.grace_days)) {
        await logActivation(env, req, license, key, storeId, fingerprint, 'error', '授权已过期');
        throw new HttpError(403, '该激活码已过期');
    }
    if (license.user_id != null) {
        const user = await env.DB.prepare('SELECT id, status FROM users WHERE id = ?').bind(license.user_id).first();
        if (user && user.status === 'disabled') {
            await logActivation(env, req, license, key, storeId, fingerprint, 'error', '用户已停用');
            throw new HttpError(403, '该授权所属账号已停用');
        }
    }
    if (fingerprint) {
        const existing = await env.DB.prepare('SELECT id, activation_count FROM devices WHERE license_id = ? AND fingerprint = ?').bind(license.id, fingerprint).first();
        if (existing) {
            const g = geo(req);
            await env.DB.prepare('UPDATE devices SET last_activated_at = ?, activation_count = activation_count + 1, last_ip = ?, country = ?, region = ?, city = ? WHERE id = ?').bind(nowIso(), ip, g.country, g.region, g.city, existing.id).run();
        } else {
            const countRow = await env.DB.prepare('SELECT COUNT(*) AS n FROM devices WHERE license_id = ?').bind(license.id).first();
            const bound = countRow?.n ?? 0;
            if (bound >= license.max_devices) {
                const reason = `绑定设备数已达上限（${license.max_devices} 台）`;
                await logActivation(env, req, license, key, storeId, fingerprint, 'error', reason);
                throw new HttpError(403, reason);
            }
            const g = geo(req);
            await env.DB.prepare('INSERT INTO devices (license_id, fingerprint, first_activated_at, last_activated_at, activation_count, last_ip, country, region, city) VALUES (?, ?, ?, ?, 1, ?, ?, ?, ?)').bind(license.id, fingerprint, nowIso(), nowIso(), ip, g.country, g.region, g.city).run();
        }
    }
    const keyId = Math.max(1, Number.parseInt(env.LICENSE_KEY_ID || '1', 10) || 1);
    const token = await signLicense(env.LICENSE_PRIVATE_KEY, buildPayload(license, fingerprint || null, settings.grace_days, keyId, env.PRODUCT || PRODUCT));
    await logActivation(env, req, license, key, storeId, fingerprint, 'ok', '');
    return corsJson({
        token,
        valid_until: license.valid_until,
        message: '激活成功，请在收银端「在线激活」中输入激活码'
    });
}
