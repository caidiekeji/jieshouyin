import { HttpError, clientIp } from './http.js';
import { nowIso } from './db.js';
import { randomHex } from './auth.js';
const CODE_TTL_SECONDS = 10 * 60;
const SEND_COOLDOWN_SECONDS = 60;
const MAX_SENDS_PER_HOUR = 5;
const MAX_IP_SENDS_PER_HOUR = 12;
const MAX_ATTEMPTS = 5;
function sixDigitCode() {
    const value = crypto.getRandomValues(new Uint32Array(1))[0] % 1_000_000;
    return String(value).padStart(6, '0');
}
async function hashCode(token, code, email, purpose) {
    const raw = new TextEncoder().encode(`${token}:${code}:${email}:${purpose}`);
    const digest = await crypto.subtle.digest('SHA-256', raw);
    let out = '';
    for (const b of new Uint8Array(digest))out += b.toString(16).padStart(2, '0');
    return out;
}
function safeEqual(a, b) {
    if (a.length !== b.length) return false;
    let diff = 0;
    for(let i = 0; i < a.length; i++)diff |= a.charCodeAt(i) ^ b.charCodeAt(i);
    return diff === 0;
}
export async function assertEmailSendAllowed(env, email, purpose, ip) {
    const now = nowIso();
    const cooldownSince = new Date(Date.now() - SEND_COOLDOWN_SECONDS * 1000).toISOString();
    const hourSince = new Date(Date.now() - 3600_000).toISOString();
    const last = await env.DB.prepare('SELECT created_at FROM email_tokens WHERE email = ? AND purpose = ? ORDER BY created_at DESC LIMIT 1').bind(email, purpose).first();
    if (last && last.created_at > cooldownSince) {
        throw new HttpError(429, '验证码发送过于频繁，请 60 秒后再试');
    }
    const hourly = await env.DB.prepare('SELECT COUNT(*) AS n FROM email_tokens WHERE email = ? AND purpose = ? AND created_at >= ?').bind(email, purpose, hourSince).first();
    if ((hourly?.n ?? 0) >= MAX_SENDS_PER_HOUR) {
        throw new HttpError(429, '该邮箱今天验证码次数过多，请稍后再试');
    }
    if (ip) {
        const byIp = await env.DB.prepare('SELECT COUNT(*) AS n FROM email_tokens WHERE ip = ? AND created_at >= ?').bind(ip, hourSince).first();
        if ((byIp?.n ?? 0) >= MAX_IP_SENDS_PER_HOUR) {
            throw new HttpError(429, '当前网络请求过于频繁，请稍后再试');
        }
    }
}
export async function createEmailCode(env, req, email, purpose, userId) {
    await assertEmailSendAllowed(env, email, purpose, clientIp(req));
    const token = randomHex(16);
    const code = sixDigitCode();
    const createdAt = nowIso();
    const expiresAt = new Date(Date.now() + CODE_TTL_SECONDS * 1000).toISOString();
    await env.DB.prepare(`INSERT INTO email_tokens (token, email, user_id, purpose, code_hash, attempts, expires_at, created_at, ip)
     VALUES (?, ?, ?, ?, ?, 0, ?, ?, ?)`).bind(token, email, userId, purpose, await hashCode(token, code, email, purpose), expiresAt, createdAt, clientIp(req)).run();
    return {
        token,
        code
    };
}
export async function deleteEmailToken(env, token) {
    await env.DB.prepare('DELETE FROM email_tokens WHERE token = ?').bind(token).run();
}
export async function verifyEmailCode(env, email, purpose, code, userId) {
    const normalized = String(code ?? '').trim();
    if (!/^\d{6}$/.test(normalized)) throw new HttpError(400, '请输入 6 位邮箱验证码');
    const row = await env.DB.prepare(`SELECT * FROM email_tokens
      WHERE email = ? AND purpose = ? AND used_at IS NULL AND expires_at > ?
      ORDER BY created_at DESC LIMIT 1`).bind(email, purpose, nowIso()).first();
    if (!row) throw new HttpError(400, '邮箱验证码不正确或已过期，请重新获取');
    if (row.attempts >= MAX_ATTEMPTS) {
        await env.DB.prepare('UPDATE email_tokens SET used_at = ? WHERE token = ?').bind(nowIso(), row.token).run();
        throw new HttpError(429, '验证码尝试次数过多，请重新获取');
    }
    if (userId !== null && row.user_id !== null && row.user_id !== userId) {
        throw new HttpError(400, '邮箱验证码与当前账号不匹配');
    }
    const expected = await hashCode(row.token, normalized, row.email, row.purpose);
    if (!safeEqual(expected, row.code_hash)) {
        const attempts = row.attempts + 1;
        await env.DB.prepare('UPDATE email_tokens SET attempts = ? WHERE token = ?').bind(attempts, row.token).run();
        throw new HttpError(400, '邮箱验证码不正确或已过期，请重新获取');
    }
    await env.DB.prepare('UPDATE email_tokens SET used_at = ? WHERE token = ?').bind(nowIso(), row.token).run();
}
export async function recordEmailEvent(env, kind, email, userId, status, detail = '', dedupeKey = '') {
    const key = dedupeKey || `${kind}:${randomHex(10)}`;
    await env.DB.prepare(`INSERT OR IGNORE INTO email_events (kind, dedupe_key, email, user_id, status, detail, created_at)
     VALUES (?, ?, ?, ?, ?, ?, ?)`).bind(kind.slice(0, 40), key.slice(0, 180), email, userId, status, detail.slice(0, 300), nowIso()).run();
}
export function emailPurposeLabel(purpose) {
    if (purpose === 'register') return '注册邮箱验证';
    if (purpose === 'login') return '登录验证码';
    if (purpose === 'reset') return '找回密码';
    return '绑定邮箱';
}
export async function cleanupEmailData(env) {
    const tokenBefore = new Date(Date.now() - 2 * 86_400_000).toISOString();
    const eventBefore = new Date(Date.now() - 90 * 86_400_000).toISOString();
    await env.DB.batch([
        env.DB.prepare('DELETE FROM email_tokens WHERE created_at < ?').bind(tokenBefore),
        env.DB.prepare('DELETE FROM email_events WHERE created_at < ?').bind(eventBefore)
    ]);
}