import { nowIso, rfc3339 } from './db.js';
import { HttpError } from './http.js';
import { makeActivationKey, makeLicenseId, licenseTypeFor, validUntilFor, isExpired } from './license.js';
export async function createLicense(env, input) {
    const issuedAt = rfc3339();
    const createdAt = nowIso();
    const validUntil = validUntilFor(input.days);
    const licenseType = licenseTypeFor(input.days);
    for(let attempt = 0; attempt < 6; attempt++){
        const key = makeActivationKey();
        const licenseId = makeLicenseId();
        try {
            const result = await env.DB.prepare(`INSERT INTO licenses
          (license_key, license_id, user_id, license_type, source, store_id, customer_name, contact,
           issued_at, valid_until, max_devices, status, note, created_by_admin_id, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'active', ?, ?, ?)`).bind(key, licenseId, input.userId, licenseType, input.source, input.storeId, input.customerName, input.contact, issuedAt, validUntil, input.maxDevices, input.note ?? '', input.adminId ?? null, createdAt).run();
            return {
                id: Number(result.meta.last_row_id ?? 0),
                license_key: key,
                license_id: licenseId,
                user_id: input.userId,
                license_type: licenseType,
                source: input.source,
                store_id: input.storeId,
                customer_name: input.customerName,
                contact: input.contact,
                issued_at: issuedAt,
                valid_until: validUntil,
                max_devices: input.maxDevices,
                status: 'active',
                note: input.note ?? '',
                created_by_admin_id: input.adminId ?? null,
                created_at: createdAt
            };
        } catch (e) {
            const msg = String(e);
            if (msg.includes('UNIQUE') && attempt < 5) continue;
            throw new HttpError(500, `创建授权失败：${msg.slice(0, 120)}`);
        }
    }
    throw new HttpError(500, '创建授权失败，请重试');
}
export async function grantFreeLicense(env, user, settings) {
    if (!settings.free_license_enabled) {
        throw new HttpError(403, '免费授权当前未开放，请联系管理员');
    }
    const existing = await env.DB.prepare(`SELECT * FROM licenses
      WHERE user_id = ? AND source = 'free' AND status = 'active'
      ORDER BY id DESC LIMIT 1`).bind(user.id).first();
    if (existing && !isExpired(existing.valid_until)) {
        return {
            license: existing,
            existed: true
        };
    }
    const license = await createLicense(env, {
        userId: user.id,
        storeId: user.store_name,
        customerName: user.customer_name,
        contact: user.contact,
        days: settings.free_license_days,
        maxDevices: settings.free_max_devices,
        source: 'free',
        note: '注册免费领取',
        adminId: null
    });
    return {
        license,
        existed: false
    };
}