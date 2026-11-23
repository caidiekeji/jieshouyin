import { json, corsJson, clientIp, HttpError } from './http.js';
import { getSettings } from './db.js';
import { serviceTime } from './license.js';
import { handleActivate } from './routes/activate.js';
import { handleRegister, handleLogin, handleLogout, handleMe, handleClaim, handleLicenses, handleChangePassword, handlePublicConfig, handleUnbindRequest, handleEmailCode, handleEmailBind, handleForgotPassword, handleResetPassword, handleLoginCode } from './routes/user.js';
import { handleBootstrap, handleAdminLogin, handleAdminLogout, handleAdminMe, handleStats, handleUsers, handleUserDetail, handleUserUpdate, handleUserPassword, handleUserDelete, handleLicenses as handleAdminLicenses, handleLicenseCreate, handleLicenseDetail, handleLicenseUpdate, handleLicenseExtend, handleLicenseBatch, handleLicenseBatchCreate, handleDeviceDelete, handleActivations, handleSettingsGet, handleSettingsUpdate, handleAdmins, handleAdminCreate, handleAdminUpdate, handleAdminPassword, handleAdminDelete, handleAudit, handleLicenseByKey, handleFreeGrant, handleEmailStatus, handleEmailTest, handleEmailRunReminders } from './routes/admin.js';
import { handleDevices, handleReleases, handleReleaseUpdate, handleReleaseDelete, handlePublicReleases, handleReleaseDownload, handleDocs, handleDocUpdate, handleDocDelete, handlePublicDocs, handlePublicFeedback, handleFeedback, handleFeedbackUpdate, handleUnbindRequests, handleUnbindRequestUpdate, handleNotifications, handleNotificationRead, handleSecurity, handleBlacklistAdd, handleBlacklistDelete, handleAccountSecurity, handleTotpSetup, handleTotpEnable, handleTotpDisable, handleSessionDelete, handleCurrentPassword, handleExport, handleBackup, handleBackupInfo, handleSearch } from './routes/ops.js';
import { runDailyMaintenance } from './reminders.js';
function num(raw) {
    const n = Number.parseInt(raw ?? '', 10);
    return Number.isFinite(n) ? n : 0;
}
async function route(req, env, url) {
    const path = url.pathname;
    const method = req.method.toUpperCase();
    if (path === '/api/status' && method === 'GET') {
        let db = false;
        try {
            await env.DB.prepare('SELECT 1 AS ok').first();
            db = true;
        } catch (e) {
            db = false;
        }
        return corsJson({
            service: 'checkout-r-activation',
            ok: db,
            db,
            time: serviceTime()
        });
    }
    if (path === '/api/activate') {
        if (method === 'OPTIONS') return corsJson({}, 204);
        if (method === 'POST') {
            try {
                return await handleActivate(req, env);
            } catch (e) {
                const status = e instanceof HttpError ? e.status : 500;
                return corsJson({
                    error: e instanceof HttpError ? e.message : '服务端处理失败'
                }, status);
            }
        }
    }
    if (path === '/api/public/config' && method === 'GET') return handlePublicConfig(env);
    if (path === '/api/public/releases' && method === 'GET') return handlePublicReleases(env);
    if (path === '/api/public/docs' && method === 'GET') return handlePublicDocs(env);
    if (path === '/api/public/feedback' && method === 'POST') return handlePublicFeedback(req, env);
    {
        const m = path.match(/^\/api\/public\/releases\/(\d+)\/download$/);
        if (m && method === 'GET') return handleReleaseDownload(req, env, num(m[1]));
    }
    if (path === '/api/user/register' && method === 'POST') return handleRegister(req, env);
    if (path === '/api/user/login' && method === 'POST') return handleLogin(req, env);
    if (path === '/api/user/login/code' && method === 'POST') return handleLoginCode(req, env);
    if (path === '/api/user/email/code' && method === 'POST') return handleEmailCode(req, env);
    if (path === '/api/user/email/bind' && method === 'POST') return handleEmailBind(req, env);
    if (path === '/api/user/password/forgot' && method === 'POST') return handleForgotPassword(req, env);
    if (path === '/api/user/password/reset' && method === 'POST') return handleResetPassword(req, env);
    if (path === '/api/user/logout' && method === 'POST') return handleLogout(req, env);
    if (path === '/api/user/me' && method === 'GET') return handleMe(req, env);
    if (path === '/api/user/claim' && method === 'POST') return handleClaim(req, env);
    if (path === '/api/user/licenses' && method === 'GET') return handleLicenses(req, env);
    if (path === '/api/user/password' && method === 'POST') return handleChangePassword(req, env);
    if (path === '/api/user/unbind-request' && method === 'POST') return handleUnbindRequest(req, env);
    if (path === '/api/admin/bootstrap' && method === 'POST') return handleBootstrap(req, env);
    if (path === '/api/admin/login' && method === 'POST') return handleAdminLogin(req, env);
    if (path === '/api/admin/logout' && method === 'POST') return handleAdminLogout(req, env);
    if (path === '/api/admin/me' && method === 'GET') return handleAdminMe(req, env);
    if (path === '/api/admin/stats' && method === 'GET') return handleStats(req, env);
    if (path === '/api/admin/users' && method === 'GET') return handleUsers(req, env, url);
    {
        const m = path.match(/^\/api\/admin\/users\/(\d+)$/);
        if (m) {
            const id = num(m[1]);
            if (method === 'GET') return handleUserDetail(req, env, id);
            if (method === 'PUT') return handleUserUpdate(req, env, id);
            if (method === 'DELETE') return handleUserDelete(req, env, id);
        }
        const p = path.match(/^\/api\/admin\/users\/(\d+)\/password$/);
        if (p && method === 'POST') return handleUserPassword(req, env, num(p[1]));
    }
    if (path === '/api/admin/licenses' && method === 'GET') return handleAdminLicenses(req, env, url);
    if (path === '/api/admin/licenses' && method === 'POST') return handleLicenseCreate(req, env);
    if (path === '/api/admin/licenses/batch' && method === 'POST') return handleLicenseBatch(req, env);
    if (path === '/api/admin/licenses/batch-create' && method === 'POST') return handleLicenseBatchCreate(req, env);
    if (path === '/api/admin/free-grant' && method === 'POST') return handleFreeGrant(req, env);
    {
        const m = path.match(/^\/api\/admin\/licenses\/(\d+)$/);
        if (m) {
            const id = num(m[1]);
            if (method === 'GET') return handleLicenseDetail(req, env, id);
            if (method === 'PUT') return handleLicenseUpdate(req, env, id);
        }
        const e = path.match(/^\/api\/admin\/licenses\/(\d+)\/extend$/);
        if (e && method === 'POST') return handleLicenseExtend(req, env, num(e[1]));
        const d = path.match(/^\/api\/admin\/licenses\/(\d+)\/devices\/(\d+)$/);
        if (d && method === 'DELETE') return handleDeviceDelete(req, env, num(d[1]), num(d[2]));
        const legacy = path.match(/^\/api\/admin\/license\/([A-Za-z0-9-]+)$/);
        if (legacy && method === 'GET') return handleLicenseByKey(req, env, legacy[1]);
    }
    if (path === '/api/admin/activations' && method === 'GET') return handleActivations(req, env, url);
    if (path === '/api/admin/settings') {
        if (method === 'GET') return handleSettingsGet(req, env);
        if (method === 'PUT') return handleSettingsUpdate(req, env);
    }
    if (path === '/api/admin/email/status' && method === 'GET') return handleEmailStatus(req, env);
    if (path === '/api/admin/email/test' && method === 'POST') return handleEmailTest(req, env);
    if (path === '/api/admin/email/reminders' && method === 'POST') return handleEmailRunReminders(req, env);
    if (path === '/api/admin/admins') {
        if (method === 'GET') return handleAdmins(req, env);
        if (method === 'POST') return handleAdminCreate(req, env);
    }
    {
        const m = path.match(/^\/api\/admin\/admins\/(\d+)$/);
        if (m) {
            const id = num(m[1]);
            if (method === 'PUT') return handleAdminUpdate(req, env, id);
            if (method === 'DELETE') return handleAdminDelete(req, env, id);
        }
        const p = path.match(/^\/api\/admin\/admins\/(\d+)\/password$/);
        if (p && method === 'POST') return handleAdminPassword(req, env, num(p[1]));
    }
    if (path === '/api/admin/audit' && method === 'GET') return handleAudit(req, env, url);
    if (path === '/api/admin/devices' && method === 'GET') return handleDevices(req, env, url);
    if (path === '/api/admin/releases') {
        if (method === 'GET' || method === 'POST') return handleReleases(req, env, url);
    }
    {
        const m = path.match(/^\/api\/admin\/releases\/(\d+)$/);
        if (m) {
            const id = num(m[1]);
            if (method === 'PUT') return handleReleaseUpdate(req, env, id);
            if (method === 'DELETE') return handleReleaseDelete(req, env, id);
        }
    }
    if (path === '/api/admin/docs') {
        if (method === 'GET' || method === 'POST') return handleDocs(req, env);
    }
    {
        const m = path.match(/^\/api\/admin\/docs\/(\d+)$/);
        if (m) {
            const id = num(m[1]);
            if (method === 'PUT') return handleDocUpdate(req, env, id);
            if (method === 'DELETE') return handleDocDelete(req, env, id);
        }
    }
    if (path === '/api/admin/feedback' && method === 'GET') return handleFeedback(req, env, url);
    {
        const m = path.match(/^\/api\/admin\/feedback\/(\d+)$/);
        if (m && method === 'PUT') return handleFeedbackUpdate(req, env, num(m[1]));
    }
    if (path === '/api/admin/requests' && method === 'GET') return handleUnbindRequests(req, env);
    {
        const m = path.match(/^\/api\/admin\/requests\/(\d+)$/);
        if (m && method === 'PUT') return handleUnbindRequestUpdate(req, env, num(m[1]));
    }
    if (path === '/api/admin/notifications' && method === 'GET') return handleNotifications(req, env, url);
    if (path === '/api/admin/notifications/read' && method === 'POST') return handleNotificationRead(req, env);
    if (path === '/api/admin/security' && method === 'GET') return handleSecurity(req, env, url);
    if (path === '/api/admin/search' && method === 'GET') return handleSearch(req, env, url);
    if (path === '/api/admin/blacklist' && method === 'POST') return handleBlacklistAdd(req, env);
    {
        const m = path.match(/^\/api\/admin\/blacklist\/(\d+)$/);
        if (m && method === 'DELETE') return handleBlacklistDelete(req, env, num(m[1]));
    }
    if (path === '/api/admin/account/security' && method === 'GET') return handleAccountSecurity(req, env, url);
    if (path === '/api/admin/account/totp/setup' && method === 'POST') return handleTotpSetup(req, env);
    if (path === '/api/admin/account/totp/enable' && method === 'POST') return handleTotpEnable(req, env);
    if (path === '/api/admin/account/totp/disable' && method === 'POST') return handleTotpDisable(req, env);
    if (path === '/api/admin/account/password' && method === 'POST') return handleCurrentPassword(req, env);
    if (path === '/api/admin/account/sessions/others' && method === 'DELETE') return handleSessionDelete(req, env, 'others');
    {
        const m = path.match(/^\/api\/admin\/account\/sessions\/([a-f0-9]{64})$/);
        if (m && method === 'DELETE') return handleSessionDelete(req, env, m[1]);
    }
    if (path === '/api/admin/export' && method === 'GET') return handleExport(req, env, url);
    if (path === '/api/admin/backup/info' && method === 'GET') return handleBackupInfo(req, env);
    if (path === '/api/admin/backup' && method === 'GET') return handleBackup(req, env);
    if (path.startsWith('/api/')) throw new HttpError(404, '接口不存在');
    return servePublicAsset(req, env, url);
}
function escapeHtml(value) {
    return value.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}
async function servePublicAsset(req, env, url) {
    let assetPath = url.pathname;
    if (assetPath === '/') assetPath = '/index.html';
    else if (!/\.[a-z0-9]+$/i.test(assetPath)) assetPath = `${assetPath.replace(/\/$/, '')}.html`;
    const assetUrl = new URL(assetPath, url.origin);
    assetUrl.search = url.search;
    const res = await env.ASSETS.fetch(new Request(assetUrl.toString(), req));
    const contentType = res.headers.get('content-type') ?? '';
    if (/\.(css|js|svg|png|jpg|jpeg|webp|woff2?|ico)$/i.test(url.pathname)) {
        const headers = new Headers(res.headers);
        headers.set('cache-control', 'public, max-age=300');
        return new Response(res.body, {
            status: res.status,
            headers
        });
    }
    if (url.pathname.startsWith('/admin')) {
        if (!contentType.includes('text/html')) return res;
        const headers = new Headers(res.headers);
        headers.set('cache-control', 'public, max-age=30');
        return new Response(res.body, {
            status: res.status,
            headers
        });
    }
    if (!contentType.includes('text/html')) return res;
    if (req.method === 'GET' && res.ok) {
        try {
            const day = new Date(Date.now() + 8 * 3600_000).toISOString().slice(0, 10);
            const path = url.pathname.slice(0, 120);
            await env.DB.prepare(`INSERT INTO page_views (day, path, n) VALUES (?, ?, 1)
         ON CONFLICT(day, path) DO UPDATE SET n = n + 1`).bind(day, path).run();
        } catch  {}
    }
    let settings;
    try {
        settings = await getSettings(env);
    } catch  {
        return res;
    }
    const siteName = settings.site_name || 'Checkout-R';
    const description = settings.site_description || '免费门店收银与激活系统';
    const keywords = settings.site_keywords || '';
    let text = await res.text();
    const pageTitle = text.match(/data-page-title="([^"]*)"/)?.[1] || siteName;
    text = text.replace(/<title>[\s\S]*?<\/title>/, `<title>${escapeHtml(pageTitle)} · ${escapeHtml(siteName)}</title>`);
    text = text.replace(/<meta name="description" content="[^"]*"/, `<meta name="description" content="${escapeHtml(description)}"`);
    if (keywords) text = text.replace(/<meta name="keywords" content="[^"]*"/, `<meta name="keywords" content="${escapeHtml(keywords)}"`);
    text = text.replace(/<meta property="og:site_name" content="[^"]*"/, `<meta property="og:site_name" content="${escapeHtml(siteName)}"`);
    text = text.replace(/<meta property="og:title" content="[^"]*"/, `<meta property="og:title" content="${escapeHtml(pageTitle)} · ${escapeHtml(siteName)}"`);
    text = text.replace(/<meta property="og:description" content="[^"]*"/, `<meta property="og:description" content="${escapeHtml(description)}"`);
    if (settings.icp_no) {
        text = text.replace('<span id="site-icp">本地数据 · 在线授权 · 永久免费</span>', `<span id="site-icp">${escapeHtml(settings.icp_no)}</span>`);
    }
    const headers = new Headers(res.headers);
    headers.delete('content-length');
    headers.set('cache-control', 'public, max-age=60');
    return new Response(text, {
        status: res.status,
        headers
    });
}
export default {
    async fetch (req, env) {
        const url = new URL(req.url);
        if (url.pathname === '/admin' || url.pathname === '/admin/' || url.pathname === '/admin.html') {
            return Response.redirect(`${url.origin}/admin/overview`, 302);
        }
        if (url.pathname === '/admin/security' || url.pathname === '/admin/security/' || url.pathname === '/admin/security.html') {
            return Response.redirect(`${url.origin}/admin/security/monitor`, 302);
        }
        if (url.pathname === '/admin/account' || url.pathname === '/admin/account/' || url.pathname === '/admin/account.html') {
            return Response.redirect(`${url.origin}/admin/account/two-factor`, 302);
        }
        if (url.pathname === '/admin/settings' || url.pathname === '/admin/settings/' || url.pathname === '/admin/settings.html') {
            return Response.redirect(`${url.origin}/admin/settings/site`, 302);
        }
        try {
            return await route(req, env, url);
        } catch (e) {
            if (e instanceof HttpError) return json({
                error: e.message
            }, e.status);
            console.error(`[${clientIp(req)}] ${req.method} ${url.pathname} -> ${String(e)}`);
            return json({
                error: '服务器内部错误'
            }, 500);
        }
    },
    async scheduled (_controller, env, ctx) {
        ctx.waitUntil(runDailyMaintenance(env));
    }
};