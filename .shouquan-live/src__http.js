export class HttpError extends Error {
    status;
    constructor(status, message){
        super(message);
        this.status = status;
    }
}
const JSON_HEADERS = {
    'Content-Type': 'application/json; charset=utf-8',
    'Cache-Control': 'no-store'
};
export function json(data, status = 200) {
    return new Response(JSON.stringify(data), {
        status,
        headers: JSON_HEADERS
    });
}
export function corsJson(data, status = 200) {
    return new Response(JSON.stringify(data), {
        status,
        headers: {
            ...JSON_HEADERS,
            'Access-Control-Allow-Origin': '*',
            'Access-Control-Allow-Headers': 'Content-Type',
            'Access-Control-Allow-Methods': 'GET, POST, OPTIONS'
        }
    });
}
export async function readJson(req) {
    const text = await req.text();
    if (!text) throw new HttpError(400, '请求体为空');
    try {
        const parsed = JSON.parse(text);
        if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
            throw new HttpError(400, '请求体必须是 JSON 对象');
        }
        return parsed;
    } catch (e) {
        if (e instanceof HttpError) throw e;
        throw new HttpError(400, '请求体不是合法 JSON');
    }
}
export function clientIp(req) {
    return (req.headers.get('CF-Connecting-IP') || req.headers.get('X-Forwarded-For') || '').trim();
}
export function userAgent(req) {
    return (req.headers.get('User-Agent') || '').slice(0, 200);
}
export function getCookie(req, name) {
    const raw = req.headers.get('Cookie') || '';
    for (const part of raw.split(';')){
        const idx = part.indexOf('=');
        if (idx < 0) continue;
        const key = part.slice(0, idx).trim();
        if (key === name) return decodeURIComponent(part.slice(idx + 1).trim());
    }
    return null;
}
export function buildCookie(req, name, value, maxAgeSeconds) {
    const secure = new URL(req.url).protocol === 'https:';
    const parts = [
        `${name}=${encodeURIComponent(value)}`,
        'Path=/',
        `Max-Age=${Math.max(0, Math.floor(maxAgeSeconds))}`,
        'HttpOnly',
        'SameSite=Lax'
    ];
    if (secure) parts.push('Secure');
    return parts.join('; ');
}
export function clearCookie(req, name) {
    return buildCookie(req, name, '', 0);
}
export function assertSameOrigin(req) {
    const origin = req.headers.get('Origin');
    if (!origin) return;
    if (origin !== new URL(req.url).origin) {
        throw new HttpError(403, '跨站请求被拒绝');
    }
}
export function withSetCookies(res, cookies) {
    for (const c of cookies)res.headers.append('Set-Cookie', c);
    return res;
}
export function clampInt(raw, fallback, min, max) {
    const n = Math.floor(Number(raw));
    if (!Number.isFinite(n)) return fallback;
    return Math.min(max, Math.max(min, n));
}
export function str(raw, maxLen) {
    return String(raw ?? '').trim().slice(0, maxLen);
}