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
 ('points_enabled','1',datetime('now')),
 ('expense_rent_yuan','0',datetime('now')),
 ('expense_utility_yuan','0',datetime('now')),
 ('expense_labor_yuan','0',datetime('now')),
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

/// M0009：行业分类模板 + 商品图片。
/// category_template 存放三套行业的分类树预设（path 用 / 分隔层级），
/// 选定时按模板写入 category 表；sku 增加本地上传图片与在线图片地址两个来源。
const M0009: &str = r#"
CREATE TABLE category_template (
    template_id INTEGER PRIMARY KEY AUTOINCREMENT,
    industry    TEXT    NOT NULL,
    path        TEXT    NOT NULL,
    sort_no     INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_category_template_industry ON category_template(industry, sort_no);

INSERT INTO category_template(industry, path, sort_no) VALUES
 ('SUPERMARKET','食品饮料',1),
 ('SUPERMARKET','食品饮料/休闲零食',2),
 ('SUPERMARKET','食品饮料/米面粮油',3),
 ('SUPERMARKET','食品饮料/调味干货',4),
 ('SUPERMARKET','食品饮料/饮料冲调',5),
 ('SUPERMARKET','食品饮料/冷冻食品',6),
 ('SUPERMARKET','食品饮料/乳品早餐',7),
 ('SUPERMARKET','生鲜果蔬',8),
 ('SUPERMARKET','生鲜果蔬/新鲜蔬菜',9),
 ('SUPERMARKET','生鲜果蔬/时令水果',10),
 ('SUPERMARKET','生鲜果蔬/肉禽蛋品',11),
 ('SUPERMARKET','生鲜果蔬/水产海鲜',12),
 ('SUPERMARKET','日用百货',13),
 ('SUPERMARKET','日用百货/纸品清洁',14),
 ('SUPERMARKET','日用百货/厨房用品',15),
 ('SUPERMARKET','日用百货/家居杂品',16),
 ('SUPERMARKET','日用百货/五金工具',17),
 ('SUPERMARKET','母婴用品',18),
 ('SUPERMARKET','母婴用品/奶粉辅食',19),
 ('SUPERMARKET','母婴用品/纸尿裤湿巾',20),
 ('SUPERMARKET','母婴用品/洗护用品',21),
 ('SUPERMARKET','个护化妆',22),
 ('SUPERMARKET','个护化妆/洗护发',23),
 ('SUPERMARKET','个护化妆/口腔护理',24),
 ('SUPERMARKET','个护化妆/彩妆护肤',25),
 ('SUPERMARKET','酒水饮料',26),
 ('SUPERMARKET','酒水饮料/白酒',27),
 ('SUPERMARKET','酒水饮料/啤酒',28),
 ('SUPERMARKET','酒水饮料/红酒洋酒',29),
 ('CLOTHING','女装',1),
 ('CLOTHING','女装/上衣T恤',2),
 ('CLOTHING','女装/衬衫针织',3),
 ('CLOTHING','女装/外套风衣',4),
 ('CLOTHING','女装/裤装',5),
 ('CLOTHING','女装/裙装',6),
 ('CLOTHING','男装',7),
 ('CLOTHING','男装/上衣T恤',8),
 ('CLOTHING','男装/衬衫',9),
 ('CLOTHING','男装/外套夹克',10),
 ('CLOTHING','男装/西裤休闲裤',11),
 ('CLOTHING','童装',12),
 ('CLOTHING','童装/婴儿连体衣',13),
 ('CLOTHING','童装/儿童上衣',14),
 ('CLOTHING','童装/儿童裤装',15),
 ('CLOTHING','童装/儿童外套',16),
 ('CLOTHING','鞋靴',17),
 ('CLOTHING','鞋靴/运动鞋',18),
 ('CLOTHING','鞋靴/男鞋',19),
 ('CLOTHING','鞋靴/女鞋',20),
 ('CLOTHING','鞋靴/童鞋',21),
 ('CLOTHING','箱包配件',22),
 ('CLOTHING','箱包配件/双肩包',23),
 ('CLOTHING','箱包配件/单肩挎包',24),
 ('CLOTHING','箱包配件/钱包',25),
 ('CLOTHING','箱包配件/帽子',26),
 ('CLOTHING','箱包配件/皮带围巾',27),
 ('CLOTHING','纺织配件',28),
 ('CLOTHING','纺织配件/内衣家居服',29),
 ('CLOTHING','纺织配件/袜子',30),
 ('CLOTHING','纺织配件/毛巾浴巾',31),
 ('RESTAURANT','经典中餐',1),
 ('RESTAURANT','经典中餐/凉菜',2),
 ('RESTAURANT','经典中餐/热菜',3),
 ('RESTAURANT','经典中餐/汤羹',4),
 ('RESTAURANT','经典中餐/主食',5),
 ('RESTAURANT','特色小吃',6),
 ('RESTAURANT','特色小吃/面食',7),
 ('RESTAURANT','特色小吃/米线粉',8),
 ('RESTAURANT','特色小吃/饺子馄饨',9),
 ('RESTAURANT','特色小吃/炸物烧烤',10),
 ('RESTAURANT','饮品',11),
 ('RESTAURANT','饮品/奶茶',12),
 ('RESTAURANT','饮品/鲜果茶',13),
 ('RESTAURANT','饮品/咖啡',14),
 ('RESTAURANT','饮品/气泡水',15),
 ('RESTAURANT','甜品',16),
 ('RESTAURANT','甜品/蛋糕',17),
 ('RESTAURANT','甜品/布丁',18),
 ('RESTAURANT','甜品/冰淇淋',19),
 ('RESTAURANT','酒水',20),
 ('RESTAURANT','酒水/啤酒',21),
 ('RESTAURANT','酒水/白酒',22),
 ('RESTAURANT','酒水/红酒',23);

ALTER TABLE sku ADD COLUMN image_path TEXT;
ALTER TABLE sku ADD COLUMN image_url TEXT;
"#;

/// M0010：会员积分累计总开关。老库无该键，OR IGNORE 补种不影响已有配置。
const M0010: &str = r#"
INSERT OR IGNORE INTO system_config(config_key, config_value, updated_at)
 VALUES ('points_enabled','1',datetime('now'));
"#;

/// M0011：利润报表月度费用（房租/水电/人工，元）。老库 OR IGNORE 补种默认 0。
const M0011: &str = r#"
INSERT OR IGNORE INTO system_config(config_key, config_value, updated_at)
 VALUES ('expense_rent_yuan','0',datetime('now')),
        ('expense_utility_yuan','0',datetime('now')),
        ('expense_labor_yuan','0',datetime('now'));
"#;

/// M0012：会员管理/充值独立权限，避免用 report.view 代理导致语义错位。
const M0012: &str = r#"
INSERT OR IGNORE INTO permission(perm_key, description) VALUES
 ('member.manage','会员建档与资料'),
 ('member.recharge','会员充值');
INSERT OR IGNORE INTO role_permission(role_id, perm_key)
SELECT r.role_id, p.perm_key FROM role r CROSS JOIN permission p
WHERE r.role_name IN ('ADMIN','MANAGER')
  AND p.perm_key IN ('member.manage','member.recharge');
"#;

/// M0013：高频查询索引（订单明细/支付/退款/库存流水/会员流水/券）。
/// SQLite 不会为外键自动建索引，万单级后详情与对账会全表扫描。
const M0013: &str = r#"
CREATE INDEX IF NOT EXISTS idx_order_item_order ON sales_order_item(order_no);
CREATE INDEX IF NOT EXISTS idx_payment_order ON payment_record(order_no);
CREATE INDEX IF NOT EXISTS idx_refund_order ON refund_record(order_no);
CREATE INDEX IF NOT EXISTS idx_refund_return ON refund_record(return_no);
CREATE INDEX IF NOT EXISTS idx_inv_trans_sku_time ON inv_trans(sku_id, created_at);
CREATE INDEX IF NOT EXISTS idx_asset_log_member_type ON member_asset_log(member_id, asset_type);
CREATE INDEX IF NOT EXISTS idx_coupon_status_until ON coupon(status, valid_until);
CREATE INDEX IF NOT EXISTS idx_return_item_order ON sales_return_item(order_item_id);
CREATE INDEX IF NOT EXISTS idx_reconcile_date_channel ON reconcile_record(recon_date, channel);
"#;

/// M0014：订单状态显式约束。SQLite 无法给已有表补 CHECK，用触发器拦截；
/// 避免任意字符串写入 status 后报表/状态机按未知状态静默跳过。
const M0014: &str = r#"
CREATE TRIGGER IF NOT EXISTS trg_order_status_ins
BEFORE INSERT ON sales_order
WHEN NEW.status NOT IN ('PENDING_PAYMENT','COMPLETED','PARTIAL_REFUND','REFUNDED','CANCELED','VOIDED')
BEGIN SELECT RAISE(ABORT,'invalid sales_order.status'); END;
CREATE TRIGGER IF NOT EXISTS trg_order_status_upd
BEFORE UPDATE OF status ON sales_order
WHEN NEW.status NOT IN ('PENDING_PAYMENT','COMPLETED','PARTIAL_REFUND','REFUNDED','CANCELED','VOIDED')
BEGIN SELECT RAISE(ABORT,'invalid sales_order.status'); END;
INSERT OR IGNORE INTO system_config(config_key,config_value,updated_at) VALUES
 ('store_no','STORE',datetime('now')),
 ('device_no','POS1',datetime('now'));
"#;

/// M0015：场景桌台（餐饮开台 / 棋牌包间）与母婴、棋牌、养生分类模板。
/// 桌台只保存未结算购物车快照，结算仍走统一 checkout，不建第二套金额口径。
const M0015: &str = r#"
ALTER TABLE sales_order ADD COLUMN table_code TEXT;
CREATE TABLE service_table (
    table_id    INTEGER PRIMARY KEY AUTOINCREMENT,
    code        TEXT    NOT NULL,
    scene       TEXT    NOT NULL,
    name        TEXT    NOT NULL,
    zone        TEXT    NOT NULL DEFAULT '',
    seats       INTEGER NOT NULL DEFAULT 4,
    fee_sku_id  INTEGER REFERENCES sku(sku_id),
    status      TEXT    NOT NULL DEFAULT 'FREE',
    created_at  TEXT    NOT NULL,
    UNIQUE(scene, code)
);
CREATE INDEX idx_service_table_scene ON service_table(scene, status);
CREATE TABLE table_session (
    session_id  INTEGER PRIMARY KEY AUTOINCREMENT,
    table_id    INTEGER NOT NULL REFERENCES service_table(table_id) ON DELETE CASCADE,
    cart_json   TEXT    NOT NULL DEFAULT '[]',
    opened_at   TEXT    NOT NULL,
    updated_at  TEXT    NOT NULL,
    status      TEXT    NOT NULL DEFAULT 'OPEN'
);
CREATE UNIQUE INDEX idx_table_session_open ON table_session(table_id) WHERE status='OPEN';
CREATE INDEX idx_table_session_time ON table_session(opened_at);

INSERT INTO category_template(industry,path,sort_no) VALUES
 ('BABY','奶粉',1),
 ('BABY','奶粉/1段',2),
 ('BABY','奶粉/2段',3),
 ('BABY','奶粉/3段',4),
 ('BABY','奶粉/羊奶粉',5),
 ('BABY','纸尿裤',6),
 ('BABY','纸尿裤/NB-S码',7),
 ('BABY','纸尿裤/M-L码',8),
 ('BABY','纸尿裤/XL-XXL码',9),
 ('BABY','辅食',10),
 ('BABY','辅食/米粉',11),
 ('BABY','辅食/果泥菜泥',12),
 ('BABY','辅食/面条粥类',13),
 ('BABY','洗护',14),
 ('BABY','洗护/沐浴护肤',15),
 ('BABY','洗护/驱蚊防晒',16),
 ('BABY','玩具',17),
 ('BABY','玩具/安抚哄睡',18),
 ('BABY','玩具/益智早教',19),
 ('BABY','童装',20),
 ('BABY','童装/内衣家居',21),
 ('BABY','童装/外出服',22),
 ('BABY','孕产',23),
 ('BABY','孕产/待产用品',24),
 ('BABY','孕产/喂养用品',25),
 ('BOARD_GAME','房费',1),
 ('BOARD_GAME','房费/大厅台',2),
 ('BOARD_GAME','房费/包间',3),
 ('BOARD_GAME','茶水饮料',4),
 ('BOARD_GAME','茶水饮料/茶',5),
 ('BOARD_GAME','茶水饮料/饮料',6),
 ('BOARD_GAME','零食',7),
 ('BOARD_GAME','零食/坚果',8),
 ('BOARD_GAME','零食/膨化',9),
 ('BOARD_GAME','烟酒',10),
 ('BOARD_GAME','烟酒/香烟',11),
 ('BOARD_GAME','烟酒/酒水',12),
 ('BOARD_GAME','棋牌用品',13),
 ('BOARD_GAME','棋牌用品/扑克',14),
 ('BOARD_GAME','棋牌用品/麻将',15),
 ('WELLNESS','推拿理疗',1),
 ('WELLNESS','推拿理疗/肩颈',2),
 ('WELLNESS','推拿理疗/腰背',3),
 ('WELLNESS','推拿理疗/全身',4),
 ('WELLNESS','艾灸',5),
 ('WELLNESS','艾灸/温和灸',6),
 ('WELLNESS','艾灸/督脉灸',7),
 ('WELLNESS','拔罐刮痧',8),
 ('WELLNESS','拔罐刮痧/拔罐',9),
 ('WELLNESS','拔罐刮痧/刮痧',10),
 ('WELLNESS','足疗',11),
 ('WELLNESS','足疗/中药泡脚',12),
 ('WELLNESS','足疗/足底按摩',13),
 ('WELLNESS','套餐',14),
 ('WELLNESS','套餐/肩颈套餐',15),
 ('WELLNESS','套餐/全身套餐',16),
 ('WELLNESS','养生产品',17),
 ('WELLNESS','养生产品/精油',18),
 ('WELLNESS','养生产品/艾条',19);
"#;

/// M0016：服务类 SKU 不计库存。房费、推拿等按次售卖的项目不进库存台账。
const M0016: &str = r#"
ALTER TABLE sku ADD COLUMN is_service INTEGER NOT NULL DEFAULT 0;
"#;

/// M0017：桌台会话补会员、已下单快照；订单补技师提成；新增技师与日排班。
const M0017: &str = r#"
CREATE TABLE IF NOT EXISTS technician (
    technician_id   INTEGER PRIMARY KEY AUTOINCREMENT,
    name            TEXT NOT NULL,
    phone           TEXT,
    commission_bps  INTEGER NOT NULL DEFAULT 0,
    status          INTEGER NOT NULL DEFAULT 1,
    created_at      TEXT NOT NULL
);
CREATE UNIQUE INDEX idx_technician_name ON technician(name);
CREATE TABLE IF NOT EXISTS technician_shift (
    technician_id  INTEGER NOT NULL REFERENCES technician(technician_id) ON DELETE CASCADE,
    business_date  TEXT NOT NULL,
    created_at     TEXT NOT NULL,
    PRIMARY KEY (technician_id, business_date)
);
CREATE INDEX idx_technician_shift_date ON technician_shift(business_date);
ALTER TABLE table_session ADD COLUMN member_id INTEGER REFERENCES member(member_id);
ALTER TABLE table_session ADD COLUMN sent_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE table_session ADD COLUMN last_ticket_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE sales_order_item ADD COLUMN technician_id INTEGER REFERENCES technician(technician_id);
ALTER TABLE sales_order_item ADD COLUMN commission_cents INTEGER NOT NULL DEFAULT 0;
"#;

/// M0018：行业规则配置化——棋牌计费单位/最低/取整、开台会员策略、
/// 技师提成方式与项目级覆盖、厨打档口路由配置。
const M0018: &str = r#"
ALTER TABLE service_table ADD COLUMN fee_unit TEXT;
ALTER TABLE service_table ADD COLUMN min_minutes INTEGER;
ALTER TABLE service_table ADD COLUMN round_minutes INTEGER;
ALTER TABLE technician ADD COLUMN commission_mode TEXT NOT NULL DEFAULT 'PERCENT';
ALTER TABLE technician ADD COLUMN fixed_cents INTEGER NOT NULL DEFAULT 0;
CREATE TABLE IF NOT EXISTS technician_commission_rule (
    technician_id  INTEGER NOT NULL REFERENCES technician(technician_id) ON DELETE CASCADE,
    category_id    INTEGER NOT NULL,
    mode           TEXT NOT NULL DEFAULT 'PERCENT',
    bps            INTEGER NOT NULL DEFAULT 0,
    fixed_cents    INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (technician_id, category_id)
);
INSERT OR IGNORE INTO system_config(config_key,config_value,updated_at) VALUES
 ('chess_fee_unit','MINUTE',datetime('now')),
 ('chess_min_minutes','60',datetime('now')),
 ('chess_round_minutes','0',datetime('now')),
 ('table_member_bind','OPTIONAL',datetime('now')),
 ('table_allow_member_rebind','0',datetime('now')),
 ('kitchen_print_routes','[]',datetime('now'));
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
        Migration {
            version: 9,
            name: "industry_category_template_product_image",
            up: M0009,
        },
        Migration {
            version: 10,
            name: "member_points_enabled",
            up: M0010,
        },
        Migration {
            version: 11,
            name: "profit_expense_config",
            up: M0011,
        },
        Migration {
            version: 12,
            name: "member_permissions",
            up: M0012,
        },
        Migration {
            version: 13,
            name: "hot_path_indexes",
            up: M0013,
        },
        Migration {
            version: 14,
            name: "order_status_guard",
            up: M0014,
        },
        Migration {
            version: 15,
            name: "service_tables_industry_templates",
            up: M0015,
        },
        Migration {
            version: 16,
            name: "sku_service_flag",
            up: M0016,
        },
        Migration {
            version: 17,
            name: "table_member_kitchen_technician",
            up: M0017,
        },
        Migration {
            version: 18,
            name: "industry_rules_configurable",
            up: M0018,
        },
    ]
}
