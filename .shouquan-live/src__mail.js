import { HttpError } from './http.js';
const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]{2,}$/;
export function normalizeEmail(raw) {
    return String(raw ?? '').trim().toLowerCase().slice(0, 120);
}
export function isValidEmail(email) {
    return EMAIL_RE.test(email);
}
function escapeHtml(value) {
    return String(value ?? '').replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}
function layout(title, intro, body) {
    return `<!doctype html>
<html lang="zh-CN"><body style="margin:0;padding:0;background:#f4f7fb;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI','Microsoft YaHei',sans-serif;color:#1c2430;">
  <div style="max-width:560px;margin:0 auto;padding:28px 18px;">
    <div style="background:#ffffff;border:1px solid #e3e9f2;border-radius:14px;overflow:hidden;">
      <div style="padding:20px 26px;border-bottom:1px solid #edf1f7;">
        <div style="font-size:13px;letter-spacing:.08em;color:#6b7a90;text-transform:uppercase;">Checkout-R</div>
      </div>
      <div style="padding:28px 26px;">
        <h1 style="margin:0 0 10px;font-size:20px;line-height:1.5;">${escapeHtml(title)}</h1>
        <p style="margin:0 0 20px;font-size:14px;line-height:1.8;color:#5c6b80;">${escapeHtml(intro)}</p>
        ${body}
      </div>
      <div style="padding:16px 26px;background:#f8fafc;border-top:1px solid #edf1f7;font-size:12px;line-height:1.7;color:#7b8aa0;">
        这是一封系统邮件，请勿直接回复。如果不是你本人操作，请忽略本邮件。
      </div>
    </div>
    <div style="padding:14px 4px 0;font-size:11px;line-height:1.6;color:#9aa8ba;">
      Checkout-R · 本地数据 · 在线授权
    </div>
  </div>
</body></html>`;
}
export async function sendMail(env, input) {
    if (!env.RESEND_API_KEY) throw new HttpError(503, '邮件服务未配置，请联系管理员');
    const from = env.MAIL_FROM || 'Checkout-R <admin@lequ.pw>';
    const recipients = (Array.isArray(input.to) ? input.to : [
        input.to
    ]).map(normalizeEmail).filter(Boolean);
    if (recipients.length === 0) throw new HttpError(400, '收件邮箱为空');
    const payload = {
        from,
        to: recipients,
        subject: input.subject.slice(0, 160),
        html: input.html
    };
    if (input.text) payload.text = input.text.slice(0, 4000);
    const replyTo = input.replyTo || env.MAIL_REPLY_TO || '';
    if (replyTo && isValidEmail(replyTo)) payload.reply_to = replyTo;
    const res = await fetch('https://api.resend.com/emails', {
        method: 'POST',
        headers: {
            Authorization: `Bearer ${env.RESEND_API_KEY}`,
            'Content-Type': 'application/json'
        },
        body: JSON.stringify(payload)
    });
    const raw = await res.text();
    let body = {};
    try {
        body = JSON.parse(raw);
    } catch  {
        body = {};
    }
    if (!res.ok || !body.id) {
        console.error(`[mail] Resend ${res.status}: ${raw.slice(0, 300)}`);
        throw new HttpError(502, '邮件发送失败，请稍后重试');
    }
    return {
        id: body.id
    };
}
export function codeMail(code, purpose) {
    const labels = {
        register: [
            '验证你的注册邮箱',
            '你正在注册 Checkout-R 门店账号，请输入下面的验证码完成邮箱验证。'
        ],
        login: [
            '登录验证码',
            '你正在登录 Checkout-R 用户中心，请输入下面的验证码完成登录。'
        ],
        reset: [
            '重置登录密码',
            '你正在找回 Checkout-R 账号密码，请输入下面的验证码继续。'
        ],
        bind: [
            '绑定邮箱验证',
            '你正在为 Checkout-R 账号绑定邮箱，请输入下面的验证码完成验证。'
        ]
    };
    const [title, intro] = labels[purpose];
    const html = layout(title, intro, `<div style="padding:18px 0 8px;text-align:center;">
      <p style="margin:0 0 8px;font-size:13px;line-height:1.6;color:#5c6b80;">你的验证码</p>
      <p style="margin:0;font-family:Consolas,'Courier New',monospace;font-size:34px;line-height:1.35;letter-spacing:8px;font-weight:700;color:#0b3b8c;">${escapeHtml(code)}</p>
      <p style="margin:14px 0 0;font-size:12px;line-height:1.7;color:#7b8aa0;">验证码 10 分钟内有效，最多尝试 5 次。</p>
    </div>`);
    return {
        subject: `${title} · Checkout-R`,
        html,
        text: `${title}\n\n${intro}\n\n验证码：${code}\n\n验证码 10 分钟内有效，最多尝试 5 次。`
    };
}
export function expiryMail(input) {
    const days = Math.max(0, input.daysLeft);
    const subject = days <= 1 ? '你的 Checkout-R 授权即将到期' : `你的 Checkout-R 授权还有 ${days} 天到期`;
    const html = layout('授权即将到期', `门店「${input.storeName || '未命名门店'}」的授权将在 ${input.validUntil.slice(0, 10)} 到期。为了避免收银端停止使用，请提前续期。`, `<div style="padding:16px;border-radius:10px;background:#f8fafc;border:1px solid #e3e9f2;">
      <div style="font-size:12px;color:#7b8aa0;margin-bottom:6px;">授权码</div>
      <div style="font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-size:16px;font-weight:600;color:#1c2430;">${escapeHtml(input.licenseKey)}</div>
      <div style="margin-top:14px;font-size:13px;color:#5c6b80;">剩余 ${days} 天 · 到期日 ${escapeHtml(input.validUntil.slice(0, 10))}</div>
    </div>
    <p style="margin:18px 0 0;font-size:13px;line-height:1.8;color:#5c6b80;">登录用户中心即可查看授权状态；如需续期，请联系管理员。</p>`);
    return {
        subject,
        html,
        text: `门店「${input.storeName || '未命名门店'}」的授权将于 ${input.validUntil.slice(0, 10)} 到期，剩余 ${days} 天。\n授权码：${input.licenseKey}`
    };
}
export function testMail() {
    const html = layout('邮件通道测试成功', '这是一封来自 Checkout-R 授权系统的测试邮件。收到即表示 Resend API Key、发件地址和域名验证都正常。', `<div style="padding:16px;border-radius:10px;background:#f0fbf4;border:1px solid #cfeeda;font-size:13px;color:#176b3a;">
      SMTP 通道：Resend · 发件域名：lequ.pw
    </div>`);
    return {
        subject: 'Checkout-R 邮件通道测试',
        html,
        text: '这是一封来自 Checkout-R 授权系统的测试邮件。收到即表示 Resend API Key、发件地址和域名验证都正常。'
    };
}
export function maskEmail(email) {
    const [name, domain] = email.split('@');
    if (!domain) return '已绑定邮箱';
    const head = name.slice(0, 1);
    return `${head}${'*'.repeat(Math.max(1, Math.min(4, name.length - 1)))}@${domain}`;
}