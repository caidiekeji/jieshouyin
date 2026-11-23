import { getSettings, nowIso } from './db.js';
import { sendMail, expiryMail } from './mail.js';
import { recordEmailEvent, cleanupEmailData } from './email-auth.js';
const DAY = 86_400_000;
export async function sendExpiryReminders(env) {
    const settings = await getSettings(env);
    if (!settings.expiry_reminder) return {
        sent: 0,
        skipped: 0
    };
    const now = Date.now();
    const until = new Date(now + settings.expiry_reminder_days * DAY).toISOString();
    const { results } = await env.DB.prepare(`SELECT l.id, l.license_key, l.valid_until, u.email, u.store_name, u.id AS user_id
       FROM licenses l JOIN users u ON u.id = l.user_id
      WHERE l.status = 'active'
        AND l.valid_until IS NOT NULL
        AND l.valid_until > ?
        AND l.valid_until <= ?
        AND u.email <> ''
        AND u.email_verified_at IS NOT NULL
      ORDER BY l.valid_until ASC
      LIMIT 100`).bind(nowIso(), until).all();
    let sent = 0;
    let skipped = 0;
    for (const row of results ?? []){
        const left = Math.max(0, Math.ceil((Date.parse(row.valid_until) - now) / DAY));
        const bucket = left <= 1 ? 1 : left <= 7 ? 7 : 30;
        const dedupeKey = `license_expiry:${row.id}:${row.valid_until.slice(0, 10)}:${bucket}`;
        const exists = await env.DB.prepare('SELECT id FROM email_events WHERE dedupe_key = ?').bind(dedupeKey).first();
        if (exists) {
            skipped++;
            continue;
        }
        try {
            await sendMail(env, {
                to: row.email,
                ...expiryMail({
                    storeName: row.store_name,
                    licenseKey: row.license_key,
                    validUntil: row.valid_until,
                    daysLeft: left
                })
            });
            await recordEmailEvent(env, 'license_expiry', row.email, row.user_id, 'ok', `${left} 天`, dedupeKey);
            sent++;
        } catch (e) {
            await recordEmailEvent(env, 'license_expiry', row.email, row.user_id, 'error', String(e).slice(0, 200));
        }
    }
    return {
        sent,
        skipped
    };
}
export async function runDailyMaintenance(env) {
    try {
        await sendExpiryReminders(env);
    } catch (e) {
        console.error(`[reminder] ${String(e)}`);
    }
    try {
        await cleanupEmailData(env);
    } catch (e) {
        console.error(`[cleanup] ${String(e)}`);
    }
}