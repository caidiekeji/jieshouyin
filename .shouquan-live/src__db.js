import { HttpError } from './http.js';
export function nowIso() {
    return new Date().toISOString();
}
export function rfc3339(date = new Date()) {
    return date.toISOString().replace(/\.\d{3}Z$/, 'Z');
}
export function addDays(days, from = new Date()) {
    return rfc3339(new Date(from.getTime() + days * 86_400_000));
}
export function shiftDays(days) {
    return new Date(Date.now() + days * 86_400_000);
}
export function isValidDateString(raw) {
    if (!raw) return false;
    const t = Date.parse(raw);
    return Number.isFinite(t);
}
const DEFAULTS = {
    registration_open: true,
    free_license_enabled: true,
    free_license_days: 365,
    free_max_devices: 1,
    grace_days: 7,
    site_notice: '',
    site_name: 'Checkout-R',
    site_keywords: '门店收银,收银系统,进销存,会员管理,激活码,Checkout-R',
    site_description: 'Checkout-R 是一套本地优先的门店收银系统。注册即可免费领取激活码，在线激活，数据保存在自己的电脑上。',
    contact_email: '',
    contact_phone: '',
    contact_address: '',
    icp_no: '',
    email_login_code: true,
    expiry_reminder: true,
    expiry_reminder_days: 7
};
export async function getSettings(env) {
    const { results } = await env.DB.prepare('SELECT key, value FROM settings').all();
    const map = new Map();
    for (const row of results ?? [])map.set(row.key, row.value);
    const flag = (key, fallback)=>{
        const v = map.get(key);
        if (v === undefined) return fallback;
        return v === '1' || v === 'true';
    };
    const num = (key, fallback, min, max)=>{
        const n = Number.parseInt(map.get(key) ?? '', 10);
        if (!Number.isFinite(n)) return fallback;
        return Math.min(max, Math.max(min, n));
    };
    return {
        registration_open: flag('registration_open', DEFAULTS.registration_open),
        free_license_enabled: flag('free_license_enabled', DEFAULTS.free_license_enabled),
        free_license_days: num('free_license_days', DEFAULTS.free_license_days, 0, 3650),
        free_max_devices: num('free_max_devices', DEFAULTS.free_max_devices, 1, 20),
        grace_days: num('grace_days', DEFAULTS.grace_days, 0, 90),
        site_notice: (map.get('site_notice') ?? DEFAULTS.site_notice).slice(0, 200),
        site_name: (map.get('site_name') ?? DEFAULTS.site_name).slice(0, 40),
        site_keywords: (map.get('site_keywords') ?? DEFAULTS.site_keywords).slice(0, 200),
        site_description: (map.get('site_description') ?? DEFAULTS.site_description).slice(0, 200),
        contact_email: (map.get('contact_email') ?? DEFAULTS.contact_email).slice(0, 120),
        contact_phone: (map.get('contact_phone') ?? DEFAULTS.contact_phone).slice(0, 40),
        contact_address: (map.get('contact_address') ?? DEFAULTS.contact_address).slice(0, 200),
        icp_no: (map.get('icp_no') ?? DEFAULTS.icp_no).slice(0, 80),
        email_login_code: flag('email_login_code', DEFAULTS.email_login_code),
        expiry_reminder: flag('expiry_reminder', DEFAULTS.expiry_reminder),
        expiry_reminder_days: num('expiry_reminder_days', DEFAULTS.expiry_reminder_days, 1, 30)
    };
}
export async function saveSettings(env, patch) {
    const statements = Object.entries(patch).map(([key, value])=>env.DB.prepare('INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value').bind(key, value));
    if (statements.length === 0) throw new HttpError(400, '没有需要保存的设置项');
    await env.DB.batch(statements);
}
export async function audit(env, actor, action, target, detail, ip) {
    const createdAt = nowIso();
    await env.DB.prepare('INSERT INTO audit_log (actor, action, target, detail, ip, created_at) VALUES (?, ?, ?, ?, ?, ?)').bind(actor, action, target.slice(0, 120), detail.slice(0, 500), ip.slice(0, 60), createdAt).run();
}