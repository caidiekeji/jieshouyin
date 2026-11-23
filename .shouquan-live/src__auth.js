import { getCookie, buildCookie, clearCookie, clientIp, HttpError } from './http.js';
import { nowIso } from './db.js';
const PBKDF2_ITERATIONS = 12_000;
export const USER_COOKIE = 'ckr_u';
export const ADMIN_COOKIE = 'ckr_a';
const USER_SESSION_SECONDS = 7 * 86_400;
const ADMIN_SESSION_SECONDS = 12 * 3600;
function toHex(bytes) {
    let out = '';
    for (const b of bytes)out += b.toString(16).padStart(2, '0');
    return out;
}
function fromHex(hex) {
    if (hex.length % 2 !== 0) throw new Error('hex 长度非法');
    const out = new Uint8Array(hex.length / 2);
    for(let i = 0; i < out.length; i++){
        out[i] = Number.parseInt(hex.slice(i * 2, i * 2 + 2), 16);
    }
    return out;
}
export function randomHex(byteLen) {
    const raw = crypto.getRandomValues(new Uint8Array(byteLen));
    return toHex(raw);
}
async function sha256Hex(text) {
    const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(text));
    return toHex(new Uint8Array(digest));
}
async function pbkdf2(password, salt, iterations) {
    const key = await crypto.subtle.importKey('raw', new TextEncoder().encode(password), 'PBKDF2', false, [
        'deriveBits'
    ]);
    const bits = await crypto.subtle.deriveBits({
        name: 'PBKDF2',
        salt: salt,
        iterations,
        hash: 'SHA-256'
    }, key, 256);
    return new Uint8Array(bits);
}
function safeEqual(a, b) {
    if (a.length !== b.length) return false;
    let diff = 0;
    for(let i = 0; i < a.length; i++)diff |= a.charCodeAt(i) ^ b.charCodeAt(i);
    return diff === 0;
}
export async function hashPassword(password) {
    const salt = crypto.getRandomValues(new Uint8Array(16));
    const hash = await pbkdf2(password, salt, PBKDF2_ITERATIONS);
    return `pbkdf2-sha256$${PBKDF2_ITERATIONS}$${toHex(salt)}$${toHex(hash)}`;
}
export async function verifyPassword(password, stored) {
    const parts = stored.split('$');
    if (parts.length !== 4 || parts[0] !== 'pbkdf2-sha256') return false;
    const iterations = Number.parseInt(parts[1], 10);
    if (!Number.isFinite(iterations) || iterations < 1) return false;
    const salt = fromHex(parts[2]);
    const expected = parts[3].toLowerCase();
    const actual = toHex(await pbkdf2(password, salt, iterations));
    return safeEqual(actual, expected);
}
export async function createSession(env, req, principal, principalId) {
    const token = randomHex(32);
    const tokenHash = await sha256Hex(token);
    const seconds = principal === 'admin' ? ADMIN_SESSION_SECONDS : USER_SESSION_SECONDS;
    const expiresAt = new Date(Date.now() + seconds * 1000).toISOString();
    await env.DB.prepare('INSERT INTO sessions (token, principal, principal_id, created_at, expires_at, ip, user_agent) VALUES (?, ?, ?, ?, ?, ?, ?)').bind(tokenHash, principal, principalId, nowIso(), expiresAt, clientIp(req).slice(0, 60), (req.headers.get('User-Agent') || '').slice(0, 200)).run();
    const name = principal === 'admin' ? ADMIN_COOKIE : USER_COOKIE;
    return {
        token,
        cookie: buildCookie(req, name, token, seconds)
    };
}
export async function sessionHash(req, principal) {
    const name = principal === 'admin' ? ADMIN_COOKIE : USER_COOKIE;
    const token = getCookie(req, name);
    return token ? sha256Hex(token) : '';
}
export async function readSession(env, req, principal) {
    const name = principal === 'admin' ? ADMIN_COOKIE : USER_COOKIE;
    const token = getCookie(req, name);
    if (!token) return null;
    const tokenHash = await sha256Hex(token);
    const row = await env.DB.prepare('SELECT principal_id, expires_at FROM sessions WHERE token = ? AND principal = ?').bind(tokenHash, principal).first();
    if (!row) return null;
    if (Date.parse(row.expires_at) <= Date.now()) {
        await env.DB.prepare('DELETE FROM sessions WHERE token = ?').bind(tokenHash).run();
        return null;
    }
    return row.principal_id;
}
export async function destroySession(env, req, principal) {
    const name = principal === 'admin' ? ADMIN_COOKIE : USER_COOKIE;
    const token = getCookie(req, name);
    if (token) {
        const tokenHash = await sha256Hex(token);
        await env.DB.prepare('DELETE FROM sessions WHERE token = ?').bind(tokenHash).run();
    }
    return clearCookie(req, name);
}
export async function assertLoginAllowed(env, scope, identifier, ip) {
    const since = new Date(Date.now() - 15 * 60_000).toISOString();
    const row = await env.DB.prepare('SELECT COUNT(*) AS n FROM login_attempts WHERE scope = ? AND created_at >= ? AND (identifier = ? OR ip = ?)').bind(scope, since, identifier.toLowerCase(), ip).first();
    if ((row?.n ?? 0) >= 10) {
        throw new HttpError(429, '登录失败次数过多，请 15 分钟后再试');
    }
}
export async function recordLoginFailure(env, scope, identifier, ip) {
    await env.DB.batch([
        env.DB.prepare('INSERT INTO login_attempts (scope, identifier, ip, created_at) VALUES (?, ?, ?, ?)').bind(scope, identifier.toLowerCase(), ip, nowIso()),
        env.DB.prepare('DELETE FROM login_attempts WHERE created_at < ?').bind(new Date(Date.now() - 86_400_000).toISOString())
    ]);
}
export async function clearLoginFailures(env, scope, identifier, ip) {
    await env.DB.prepare('DELETE FROM login_attempts WHERE scope = ? AND (identifier = ? OR ip = ?)').bind(scope, identifier.toLowerCase(), ip).run();
}
export const PASSWORD_RULE = /^(?=.*[A-Za-z0-9]).{8,64}$/;