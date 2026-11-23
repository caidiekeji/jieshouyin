const ALPHABET = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567';
function base32Encode(bytes) {
    let out = '';
    let bits = 0;
    let value = 0;
    for (const b of bytes){
        value = value << 8 | b;
        bits += 8;
        while(bits >= 5){
            out += ALPHABET[value >>> bits - 5 & 31];
            bits -= 5;
        }
    }
    if (bits > 0) out += ALPHABET[value << 5 - bits & 31];
    return out;
}
function base32Decode(input) {
    const clean = input.toUpperCase().replace(/[^A-Z2-7]/g, '');
    const out = [];
    let bits = 0;
    let value = 0;
    for (const ch of clean){
        const idx = ALPHABET.indexOf(ch);
        if (idx < 0) continue;
        value = value << 5 | idx;
        bits += 5;
        if (bits >= 8) {
            out.push(value >>> bits - 8 & 255);
            bits -= 8;
        }
    }
    return new Uint8Array(out);
}
function toHex(bytes) {
    let out = '';
    for (const b of bytes)out += b.toString(16).padStart(2, '0');
    return out;
}
export function generateTotpSecret() {
    const bytes = crypto.getRandomValues(new Uint8Array(20));
    return base32Encode(bytes);
}
async function hotp(secret, counter) {
    const key = await crypto.subtle.importKey('raw', base32Decode(secret), {
        name: 'HMAC',
        hash: 'SHA-1'
    }, false, [
        'sign'
    ]);
    const buf = new ArrayBuffer(8);
    const view = new DataView(buf);
    view.setUint32(0, Math.floor(counter / 2 ** 32));
    view.setUint32(4, counter % 2 ** 32);
    const sig = new Uint8Array(await crypto.subtle.sign('HMAC', key, buf));
    const offset = sig[sig.length - 1] & 0x0f;
    const code = (sig[offset] & 0x7f) << 24 | (sig[offset + 1] & 0xff) << 16 | (sig[offset + 2] & 0xff) << 8 | sig[offset + 3] & 0xff;
    return String(code % 1_000_000).padStart(6, '0');
}
export async function currentTotp(secret, at = Date.now()) {
    return hotp(secret, Math.floor(at / 30_000));
}
export async function verifyTotp(secret, token, window = 1) {
    const code = String(token || '').replace(/\D/g, '');
    if (!secret || code.length !== 6) return false;
    const now = Math.floor(Date.now() / 30_000);
    for(let i = -window; i <= window; i++){
        if (await hotp(secret, now + i) === code) return true;
    }
    return false;
}
export function otpauthUrl(secret, account) {
    const label = encodeURIComponent(`Checkout-R:${account}`);
    const params = new URLSearchParams({
        secret,
        issuer: 'Checkout-R',
        algorithm: 'SHA1',
        digits: '6',
        period: '30'
    });
    return `otpauth://totp/${label}?${params.toString()}`;
}
export { base32Encode, toHex };