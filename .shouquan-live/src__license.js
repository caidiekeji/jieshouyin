import { rfc3339, addDays } from './db.js';
export const PRODUCT = 'checkout-r';
export const ENTITLEMENTS = [
    'pos.core',
    'report.standard',
    'member.basic'
];
function base64UrlEncode(bytes) {
    let bin = '';
    for (const b of bytes)bin += String.fromCharCode(b);
    return btoa(bin).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}
function hexToBytes(hex) {
    const clean = hex.trim().toLowerCase();
    if (clean.length % 2 !== 0) throw new Error('hex 长度非法');
    const out = new Uint8Array(clean.length / 2);
    for(let i = 0; i < out.length; i++){
        const byte = Number.parseInt(clean.slice(i * 2, i * 2 + 2), 16);
        if (Number.isNaN(byte)) throw new Error('hex 内容非法');
        out[i] = byte;
    }
    return out;
}
const PKCS8_ED25519_PREFIX = new Uint8Array([
    0x30,
    0x2e,
    0x02,
    0x01,
    0x00,
    0x30,
    0x05,
    0x06,
    0x03,
    0x2b,
    0x65,
    0x70,
    0x04,
    0x22,
    0x04,
    0x20
]);
async function importPrivateKey(seedHex) {
    const seed = hexToBytes(seedHex);
    if (seed.length !== 32) throw new Error('Ed25519 私钥必须是 32 字节（64 位 hex）');
    const der = new Uint8Array(PKCS8_ED25519_PREFIX.length + 32);
    der.set(PKCS8_ED25519_PREFIX, 0);
    der.set(seed, PKCS8_ED25519_PREFIX.length);
    return crypto.subtle.importKey('pkcs8', der, {
        name: 'Ed25519'
    }, false, [
        'sign'
    ]);
}
export async function signLicense(privateKeyHex, payload) {
    const key = await importPrivateKey(privateKeyHex);
    const json = JSON.stringify(payload);
    const sig = await crypto.subtle.sign({
        name: 'Ed25519'
    }, key, new TextEncoder().encode(json));
    return `${base64UrlEncode(new TextEncoder().encode(json))}.${base64UrlEncode(new Uint8Array(sig))}`;
}
export function buildPayload(rec, fingerprint, graceDays, product = PRODUCT) {
    return {
        license_id: rec.license_id,
        product,
        license_type: rec.license_type,
        issued_at: rec.issued_at,
        valid_until: rec.valid_until,
        maintenance_until: rec.license_type === 'PERPETUAL' ? null : rec.valid_until,
        grace_period_days: graceDays,
        max_devices: rec.max_devices,
        device_fingerprint: fingerprint,
        entitlements: [
            ...ENTITLEMENTS
        ],
        store_id: rec.store_id,
        customer_name: rec.customer_name
    };
}
const KEY_ALPHABET = 'ABCDEFGHJKLMNPQRSTUVWXYZ23456789';
export function makeActivationKey() {
    const raw = crypto.getRandomValues(new Uint8Array(16));
    let body = '';
    for (const b of raw)body += KEY_ALPHABET[b % KEY_ALPHABET.length];
    return `CKR-${body.slice(0, 4)}-${body.slice(4, 8)}-${body.slice(8, 12)}-${body.slice(12, 16)}`;
}
export function makeLicenseId() {
    return `lic_${Date.now().toString(36)}${randomToken()}`;
}
function randomToken() {
    const raw = crypto.getRandomValues(new Uint8Array(6));
    let out = '';
    for (const b of raw)out += (b % 36).toString(36);
    return out;
}
export function isExpired(validUntil, now = Date.now()) {
    if (!validUntil) return false;
    return Date.parse(validUntil) < now;
}
export function withinGrace(validUntil, graceDays, now = Date.now()) {
    if (!validUntil) return false;
    return Date.parse(validUntil) + graceDays * 86_400_000 < now;
}
export function licenseTypeFor(days) {
    return days > 0 ? 'ANNUAL' : 'PERPETUAL';
}
export function validUntilFor(days) {
    return days > 0 ? addDays(days) : null;
}
export function displayValidUntil(validUntil) {
    return validUntil ? validUntil.slice(0, 10) : '永久';
}
export function serviceTime() {
    return rfc3339();
}