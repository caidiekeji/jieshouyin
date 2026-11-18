//! §11.3 v1 迁移序列：0001–0006。每个迁移一个独立事务。

use super::Migration;

const M0001: &str = r#"
CREATE TABLE system_config (
    config_key   TEXT PRIMARY KEY,
    config_value TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);
CREATE TABLE spu (
    spu_id      INTEGER PRIMARY KEY AUTOINCREMENT,
    category_id INTEGER NOT NULL,
    spu_name    TEXT NOT NULL,
    spu_code    TEXT NOT NULL UNIQUE,
    unit        TEXT NOT NULL DEFAULT '件',
    status      INTEGER NOT NULL DEFAULT 1
);
CREATE TABLE sku (
    sku_id               INTEGER PRIMARY KEY AUTOINCREMENT,
    spu_id               INTEGER NOT NULL REFERENCES spu(spu_id),
    barcode              TEXT UNIQUE,
    instore_code         TEXT UNIQUE,
    spec_json            TEXT,
    sale_price_cents     INTEGER NOT NULL,
    cost_price_cents     INTEGER NOT NULL DEFAULT 0,
    member_price_cents   INTEGER,
    is_weighed           INTEGER NOT NULL DEFAULT 0,
    allow_negative_stock INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX idx_sku_spu ON sku(spu_id);
CREATE TABLE inv_stock (
    stock_id       INTEGER PRIMARY KEY AUTOINCREMENT,
    sku_id         INTEGER NOT NULL REFERENCES sku(sku_id),
    warehouse_type TEXT    NOT NULL DEFAULT 'STORE',
    qty            INTEGER NOT NULL DEFAULT 0,
    locked_qty     INTEGER NOT NULL DEFAULT 0,
    version        INTEGER NOT NULL DEFAULT 0,
    UNIQUE(sku_id, warehouse_type)
);
CREATE TABLE inv_trans (
    trans_id     INTEGER PRIMARY KEY AUTOINCREMENT,
    sku_id       INTEGER NOT NULL,
    warehouse_type TEXT NOT NULL,
    change_qty   INTEGER NOT NULL,
    trans_type   TEXT NOT NULL,
    ref_order_no TEXT,
    operator_id  INTEGER,
    created_at   TEXT NOT NULL
);
CREATE TABLE sales_order (
    order_no       TEXT PRIMARY KEY,
    device_id      TEXT NOT NULL,
    cashier_id     INTEGER NOT NULL,
    shift_id       INTEGER NOT NULL,
    member_id      INTEGER,
    total_cents    INTEGER NOT NULL,
    discount_cents INTEGER NOT NULL DEFAULT 0,
    pay_cents      INTEGER NOT NULL,
    status         TEXT NOT NULL DEFAULT 'PENDING_PAYMENT',
    business_date  TEXT NOT NULL,
    sync_status    TEXT NOT NULL DEFAULT 'LOCAL_ONLY',
    created_at     TEXT NOT NULL,
    synced_at      TEXT
);
CREATE INDEX idx_order_business ON sales_order(business_date);
CREATE INDEX idx_order_sync ON sales_order(sync_status);
CREATE INDEX idx_order_status ON sales_order(status);
CREATE TABLE sales_order_item (
    item_id          INTEGER PRIMARY KEY AUTOINCREMENT,
    order_no         TEXT NOT NULL REFERENCES sales_order(order_no),
    sku_id           INTEGER NOT NULL,
    qty              INTEGER NOT NULL,
    unit_price_cents INTEGER NOT NULL,
    line_total_cents INTEGER NOT NULL,
    discount_cents   INTEGER NOT NULL DEFAULT 0,
    returned_qty     INTEGER NOT NULL DEFAULT 0,
    is_gift          INTEGER NOT NULL DEFAULT 0,
    gift_promo_id    INTEGER
);
CREATE TABLE parked_order (
    park_id     INTEGER PRIMARY KEY AUTOINCREMENT,
    cart_json   TEXT NOT NULL,
    cashier_id  INTEGER NOT NULL,
    member_id   INTEGER,
    remark      TEXT,
    status      TEXT NOT NULL DEFAULT 'PENDING',
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);
CREATE TABLE sales_return (
    return_no          TEXT PRIMARY KEY,
    order_no           TEXT NOT NULL REFERENCES sales_order(order_no),
    member_id          INTEGER,
    cashier_id         INTEGER NOT NULL,
    approver_id        INTEGER,
    refund_total_cents INTEGER NOT NULL,
    status             TEXT NOT NULL DEFAULT 'PENDING_REFUND',
    created_at         TEXT NOT NULL
);
CREATE TABLE sales_return_item (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    return_no     TEXT NOT NULL REFERENCES sales_return(return_no),
    order_item_id INTEGER NOT NULL REFERENCES sales_order_item(item_id),
    sku_id        INTEGER NOT NULL,
    qty           INTEGER NOT NULL,
    refund_cents  INTEGER NOT NULL
);
CREATE TABLE shift (
    shift_id             INTEGER PRIMARY KEY AUTOINCREMENT,
    device_id            TEXT NOT NULL,
    cashier_ids_json     TEXT NOT NULL,
    opened_at            TEXT NOT NULL,
    closed_at            TEXT,
    opening_float_cents  INTEGER NOT NULL DEFAULT 0,
    expected_cash_cents  INTEGER,
    actual_cash_cents    INTEGER,
    difference_cents     INTEGER,
    difference_reason    TEXT,
    status               TEXT NOT NULL DEFAULT 'OPEN'
);
INSERT INTO system_config(config_key, config_value, updated_at) VALUES
 ('store_timezone','Asia/Shanghai',datetime('now')),
 ('business_day_cutoff','00:00',datetime('now')),
 ('scan_gap_ms','50',datetime('now')),
 ('micropay_poll_ms','2000',datetime('now')),
 ('micropay_timeout_s','60',datetime('now')),
 ('native_poll_ms','3000',datetime('now')),
 ('native_timeout_s','300',datetime('now')),
 ('allow_negative_stock_default','1',datetime('now')),
 ('points_per_yuan','1',datetime('now')),
 ('points_redeem_rate','100',datetime('now')),
 ('max_discount_ratio','0.8',datetime('now'));
"#;

const M0002: &str = r#"
CREATE TABLE license_local (
    license_id          TEXT PRIMARY KEY,
    payload_json        TEXT NOT NULL,
    signature           TEXT NOT NULL,
    device_fingerprint  TEXT,
    activated_at        TEXT NOT NULL,
    last_known_time     TEXT,
    last_verified_at    TEXT
);
CREATE TABLE license_audit (
    audit_id            INTEGER PRIMARY KEY AUTOINCREMENT,
    event_type          TEXT NOT NULL,
    license_id          TEXT,
    payload_hash        TEXT,
    device_fp_digest    TEXT,
    detail_json         TEXT,
    created_at          TEXT NOT NULL
);
CREATE TABLE license_state (
    state_key           TEXT PRIMARY KEY,
    state               TEXT NOT NULL,
    license_type        TEXT NOT NULL,
    entitlements_json   TEXT NOT NULL,
    valid_until         TEXT,
    maintenance_until   TEXT,
    updated_at          TEXT NOT NULL
);
CREATE TABLE role (
    role_id   INTEGER PRIMARY KEY AUTOINCREMENT,
    role_name TEXT NOT NULL UNIQUE
);
CREATE TABLE permission (
    perm_key    TEXT PRIMARY KEY,
    description TEXT
);
CREATE TABLE role_permission (
    role_id  INTEGER NOT NULL REFERENCES role(role_id),
    perm_key TEXT    NOT NULL REFERENCES permission(perm_key),
    PRIMARY KEY (role_id, perm_key)
);
CREATE TABLE role_limit (
    role_id   INTEGER NOT NULL REFERENCES role(role_id),
    limit_key TEXT    NOT NULL,
    max_cents INTEGER,
    PRIMARY KEY (role_id, limit_key)
);
CREATE TABLE user_account (
    user_id       INTEGER PRIMARY KEY AUTOINCREMENT,
    username      TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    role_id       INTEGER NOT NULL REFERENCES role(role_id),
    status        INTEGER NOT NULL DEFAULT 1,
    failed_attempts INTEGER NOT NULL DEFAULT 0,
    locked_until  TEXT
);
CREATE TABLE operation_audit (
    audit_id     INTEGER PRIMARY KEY AUTOINCREMENT,
    command      TEXT NOT NULL,
    operator_id  INTEGER,
    approver_id  INTEGER,
    detail_json  TEXT,
    created_at   TEXT NOT NULL
);
INSERT INTO role(role_name) VALUES ('CASHIER'),('MANAGER'),('ADMIN');
INSERT INTO permission(perm_key, description) VALUES
 ('pos.checkout','前台开单'),
 ('pos.void','订单作废'),
 ('pos.refund','退款'),
 ('pos.discount.manual','手工折扣'),
 ('pos.price_override','改价'),
 ('pos.shift.close','交接班'),
 ('product.edit','商品维护'),
 ('inventory.adjust','库存调整'),
 ('purchase.create','采购单'),
 ('stocktake.approve','盘点审批'),
 ('report.view','查看报表'),
 ('report.export','导出报表'),
 ('system.config','系统配置'),
 ('user.manage','用户管理');
INSERT INTO role_permission(role_id, perm_key)
SELECT r.role_id, p.perm_key FROM role r CROSS JOIN permission p
WHERE r.role_name='ADMIN';
INSERT INTO role_permission(role_id, perm_key)
SELECT r.role_id, p.perm_key FROM role r CROSS JOIN permission p
WHERE r.role_name='MANAGER' AND p.perm_key <> 'report.export'
  AND p.perm_key <> 'system.config' AND p.perm_key <> 'user.manage';
INSERT INTO role_permission(role_id, perm_key)
SELECT r.role_id, p.perm_key FROM role r CROSS JOIN permission p
WHERE r.role_name='CASHIER' AND p.perm_key IN
 ('pos.checkout','pos.shift.close','pos.refund');
INSERT INTO role_limit(role_id, limit_key, max_cents)
SELECT r.role_id,'refund.max_cents',5000 FROM role r WHERE r.role_name='CASHIER';
INSERT INTO role_limit(role_id, limit_key, max_cents)
SELECT r.role_id,'price_override.max_cents',2000 FROM role r WHERE r.role_name='MANAGER';
"#;

const M0003: &str = r#"
CREATE TABLE member (
    member_id   INTEGER PRIMARY KEY AUTOINCREMENT,
    phone       TEXT UNIQUE,
    member_code TEXT UNIQUE,
    name        TEXT,
    level_id    INTEGER,
    status      INTEGER NOT NULL DEFAULT 1
);
CREATE TABLE member_asset (
    member_id  INTEGER NOT NULL REFERENCES member(member_id),
    asset_type TEXT    NOT NULL,
    balance    INTEGER NOT NULL DEFAULT 0,
    version    INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (member_id, asset_type)
);
CREATE TABLE member_asset_log (
    log_id           INTEGER PRIMARY KEY AUTOINCREMENT,
    member_id        INTEGER NOT NULL,
    asset_type       TEXT    NOT NULL,
    change_amount    INTEGER NOT NULL,
    balance_after    INTEGER NOT NULL,
    reason           TEXT    NOT NULL,
    ref_order_no     TEXT,
    idempotency_key  TEXT    UNIQUE,
    created_at       TEXT    NOT NULL
);
CREATE TABLE coupon (
    coupon_id       INTEGER PRIMARY KEY AUTOINCREMENT,
    member_id       INTEGER,
    coupon_code     TEXT UNIQUE,
    discount_cents  INTEGER NOT NULL,
    min_order_cents INTEGER NOT NULL DEFAULT 0,
    status          TEXT NOT NULL DEFAULT 'UNUSED',
    valid_from      TEXT,
    valid_until     TEXT
);
"#;

const M0004: &str = r#"
CREATE TABLE payment_record (
    pay_id               INTEGER PRIMARY KEY AUTOINCREMENT,
    order_no             TEXT    NOT NULL,
    pay_channel          TEXT    NOT NULL,
    pay_mode             TEXT,
    attempt_no           INTEGER NOT NULL DEFAULT 1,
    amount_cents         INTEGER NOT NULL,
    cash_tendered_cents  INTEGER,
    change_cents         INTEGER,
    sv_bonus_cents       INTEGER,
    sv_principal_cents   INTEGER,
    out_trade_no         TEXT    UNIQUE,
    trade_no             TEXT,
    status               TEXT    NOT NULL DEFAULT 'PENDING',
    paid_at              TEXT,
    created_at           TEXT    NOT NULL
);
CREATE TABLE refund_record (
    refund_id         INTEGER PRIMARY KEY AUTOINCREMENT,
    pay_id            INTEGER NOT NULL REFERENCES payment_record(pay_id),
    return_no         TEXT,
    order_no          TEXT    NOT NULL,
    refund_no         TEXT    NOT NULL UNIQUE,
    amount_cents      INTEGER NOT NULL,
    reason            TEXT,
    status            TEXT    NOT NULL DEFAULT 'PROCESSING',
    channel_refund_no TEXT,
    created_at        TEXT    NOT NULL,
    updated_at        TEXT    NOT NULL
);
CREATE TABLE reconcile_record (
    recon_id             INTEGER PRIMARY KEY AUTOINCREMENT,
    recon_date           TEXT NOT NULL,
    channel              TEXT NOT NULL,
    local_amount_cents   INTEGER NOT NULL,
    channel_amount_cents INTEGER NOT NULL,
    diff_type            TEXT NOT NULL,
    order_no             TEXT,
    detail_json          TEXT,
    created_at           TEXT NOT NULL
);
CREATE TABLE platform_cert (
    serial          TEXT PRIMARY KEY,
    public_key_pem  TEXT NOT NULL,
    expires_at      TEXT NOT NULL
);
"#;

const M0005: &str = r#"
CREATE TABLE supplier (
    supplier_id     INTEGER PRIMARY KEY AUTOINCREMENT,
    name            TEXT    NOT NULL,
    contact         TEXT,
    phone           TEXT,
    status          INTEGER NOT NULL DEFAULT 1
);
CREATE TABLE purchase_order (
    po_no      TEXT PRIMARY KEY,
    supplier_id INTEGER NOT NULL REFERENCES supplier(supplier_id),
    status     TEXT NOT NULL DEFAULT 'DRAFT',
    total_cents INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL
);
CREATE TABLE purchase_order_item (
    item_id         INTEGER PRIMARY KEY AUTOINCREMENT,
    po_no           TEXT NOT NULL REFERENCES purchase_order(po_no),
    sku_id          INTEGER NOT NULL,
    ordered_qty     INTEGER NOT NULL,
    received_qty    INTEGER NOT NULL DEFAULT 0,
    unit_cost_cents INTEGER NOT NULL
);
CREATE TABLE purchase_receipt (
    receipt_id  INTEGER PRIMARY KEY AUTOINCREMENT,
    po_no       TEXT NOT NULL,
    received_at TEXT NOT NULL,
    operator_id INTEGER NOT NULL
);
CREATE TABLE purchase_receipt_diff (
    diff_id      INTEGER PRIMARY KEY AUTOINCREMENT,
    receipt_id   INTEGER NOT NULL REFERENCES purchase_receipt(receipt_id),
    sku_id       INTEGER NOT NULL,
    ordered_qty  INTEGER NOT NULL,
    received_qty INTEGER NOT NULL,
    diff_qty     INTEGER NOT NULL,
    reason       TEXT
);
CREATE TABLE stocktake (
    stocktake_id INTEGER PRIMARY KEY AUTOINCREMENT,
    task_no      TEXT NOT NULL UNIQUE,
    status       TEXT NOT NULL DEFAULT 'DRAFT',
    operator_id  INTEGER NOT NULL,
    approver_id  INTEGER,
    started_at   TEXT,
    finished_at  TEXT,
    created_at   TEXT NOT NULL
);
CREATE TABLE stocktake_item (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    stocktake_id INTEGER NOT NULL REFERENCES stocktake(stocktake_id),
    sku_id       INTEGER NOT NULL,
    book_qty     INTEGER NOT NULL,
    counted_qty  INTEGER,
    diff_qty     INTEGER,
    reason       TEXT
);
"#;

const M0006: &str = r#"
CREATE TABLE promotion (
    promotion_id    INTEGER PRIMARY KEY AUTOINCREMENT,
    name            TEXT    NOT NULL,
    promo_type      TEXT    NOT NULL,
    scope           TEXT    NOT NULL,
    priority        INTEGER NOT NULL DEFAULT 100,
    exclusive       INTEGER NOT NULL DEFAULT 0,
    stackable       INTEGER NOT NULL DEFAULT 0,
    condition_json  TEXT    NOT NULL,
    action_json     TEXT    NOT NULL,
    valid_from      TEXT,
    valid_until     TEXT,
    status          INTEGER NOT NULL DEFAULT 1
);
CREATE TABLE order_promotion (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    order_no        TEXT    NOT NULL,
    promotion_id    INTEGER NOT NULL,
    discount_cents  INTEGER NOT NULL,
    detail_json     TEXT
);
CREATE TABLE print_template (
    template_id    TEXT PRIMARY KEY,
    paper_width_mm INTEGER NOT NULL,
    content_json   TEXT NOT NULL,
    is_default     INTEGER NOT NULL DEFAULT 0,
    updated_at     TEXT NOT NULL
);
CREATE TABLE print_retry_queue (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    order_no    TEXT NOT NULL,
    template_id TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'PENDING',
    retry_count INTEGER NOT NULL DEFAULT 0,
    last_error  TEXT,
    created_at  TEXT NOT NULL
);
CREATE TABLE master_changelog (
    version     INTEGER PRIMARY KEY AUTOINCREMENT,
    entity_type TEXT    NOT NULL,
    entity_id   TEXT    NOT NULL,
    patch_json  TEXT    NOT NULL,
    created_at  TEXT    NOT NULL
);
CREATE TABLE sync_watermark (
    entity_type  TEXT PRIMARY KEY,
    last_version INTEGER NOT NULL DEFAULT 0,
    updated_at   TEXT NOT NULL
);
CREATE TABLE sync_queue (
    op_id        TEXT PRIMARY KEY,
    op_type      TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    base_version INTEGER,
    status       TEXT NOT NULL DEFAULT 'PENDING',
    retry_count  INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL
);
CREATE TABLE daily_summary (
    business_date     TEXT PRIMARY KEY,
    order_count       INTEGER NOT NULL,
    total_sales_cents INTEGER NOT NULL,
    refund_cents      INTEGER NOT NULL,
    discount_cents    INTEGER NOT NULL,
    generated_at      TEXT NOT NULL
);
INSERT INTO print_template(template_id, paper_width_mm, content_json, is_default, updated_at) VALUES
 ('receipt_default', 58,
  '{"template_id":"receipt_default","sections":{"header":[{"type":"text","content":"{{store_name}}","align":"center","bold":true},{"type":"text","content":"{{store_phone}}","align":"center"}],"items":[{"type":"text","content":"{{items}}"}],"summary":[{"type":"divider"},{"type":"text","content":"合计 {{pay_total}}","align":"right"},{"type":"text","content":"找零 {{change}}","align":"right"}],"footer":[{"type":"qrcode","content":"{{qrcode_pay_url}}","size":4},{"type":"feed","lines":2},{"type":"cut","mode":"partial"},{"type":"kick","pin":0}]}}',
  1, datetime('now')),
 ('shift_default', 58,
  '{"template_id":"shift_default","sections":{"header":[{"type":"text","content":"交接班汇总","align":"center","bold":true}],"items":[{"type":"text","content":"{{items}}"}],"summary":[{"type":"divider"},{"type":"text","content":"现金差额 {{change}}"}] ,"footer":[{"type":"feed","lines":2},{"type":"cut","mode":"partial"}]}}',
  0, datetime('now'));
"#;

/// 0007：商品管理补全（F-11/F-12/F-15）——多级类目表、库存预警阈值。
const M0007: &str = r#"
CREATE TABLE category (
    category_id INTEGER PRIMARY KEY AUTOINCREMENT,
    parent_id   INTEGER REFERENCES category(category_id),
    name        TEXT    NOT NULL,
    status      INTEGER NOT NULL DEFAULT 1
);
INSERT INTO category(category_id, parent_id, name) VALUES (1, NULL, '未分类');
ALTER TABLE sku ADD COLUMN alert_qty INTEGER NOT NULL DEFAULT 0;
"#;

/// M0008：订单记录券码（Native 支付/崩溃恢复的 finalize 收口需要）。
const M0008: &str = r#"
ALTER TABLE sales_order ADD COLUMN coupon_code TEXT;
"#;

pub fn all() -> Vec<Migration> {
    vec![
        Migration {
            version: 1,
            name: "init_core",
            up: M0001,
        },
        Migration {
            version: 2,
            name: "license_rbac",
            up: M0002,
        },
        Migration {
            version: 3,
            name: "member",
            up: M0003,
        },
        Migration {
            version: 4,
            name: "payment",
            up: M0004,
        },
        Migration {
            version: 5,
            name: "purchase_stocktake",
            up: M0005,
        },
        Migration {
            version: 6,
            name: "promo_print_report",
            up: M0006,
        },
        Migration {
            version: 7,
            name: "category_alert_qty",
            up: M0007,
        },
        Migration {
            version: 8,
            name: "order_coupon_code",
            up: M0008,
        },
    ]
}
