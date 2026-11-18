//! 订单查询视图：订单列表（退款页检索）与订单详情（明细/支付/优惠/退货）。
//!
//! 只读查询，不写库；金额口径与 order.rs 一致（分）。

use crate::error::Result;
use rusqlite::{params, Connection, OptionalExtension};

/// 订单摘要。
#[derive(Debug, Clone, serde::Serialize)]
pub struct OrderSummary {
    pub order_no: String,
    pub business_date: String,
    pub created_at: String,
    pub cashier_id: i64,
    pub member_id: Option<i64>,
    pub member_name: Option<String>,
    pub total_cents: i64,
    pub discount_cents: i64,
    pub pay_cents: i64,
    pub status: String,
    pub item_count: i64,
    /// 已退金额合计。
    pub refunded_cents: i64,
}

/// 订单明细行。
#[derive(Debug, Clone, serde::Serialize)]
pub struct OrderItemDetail {
    pub item_id: i64,
    pub sku_id: i64,
    pub product_name: String,
    pub qty: i64,
    pub returned_qty: i64,
    pub unit_price_cents: i64,
    pub line_total_cents: i64,
    pub discount_cents: i64,
    pub is_gift: bool,
}

/// 支付流水明细。
#[derive(Debug, Clone, serde::Serialize)]
pub struct PaymentDetail {
    pub pay_id: i64,
    pub pay_channel: String,
    pub pay_mode: Option<String>,
    pub amount_cents: i64,
    pub cash_tendered_cents: Option<i64>,
    pub change_cents: Option<i64>,
    pub out_trade_no: Option<String>,
    pub trade_no: Option<String>,
    pub status: String,
    pub paid_at: Option<String>,
    pub created_at: String,
}

/// 优惠明细。
#[derive(Debug, Clone, serde::Serialize)]
pub struct PromoDetail {
    pub promotion_id: i64,
    pub discount_cents: i64,
    pub detail: Option<String>,
}

/// 退货记录。
#[derive(Debug, Clone, serde::Serialize)]
pub struct RefundDetail {
    pub refund_id: i64,
    pub return_no: Option<String>,
    pub refund_no: String,
    pub amount_cents: i64,
    pub status: String,
    pub created_at: String,
}

/// 订单详情全量视图。
#[derive(Debug, Clone, serde::Serialize)]
pub struct OrderDetail {
    pub header: OrderSummary,
    pub items: Vec<OrderItemDetail>,
    pub payments: Vec<PaymentDetail>,
    pub promos: Vec<PromoDetail>,
    pub refunds: Vec<RefundDetail>,
}

/// 订单列表：按业务日过滤 + 单号/关键字模糊匹配。
/// date 为空表示不限日期；keyword 为空表示不过滤。
pub fn list_orders(
    conn: &Connection,
    date: Option<&str>,
    keyword: &str,
    limit: i64,
) -> Result<Vec<OrderSummary>> {
    let pattern = format!("%{keyword}%");
    let mut stmt = conn.prepare(
        "SELECT o.order_no, o.business_date, o.created_at, o.cashier_id, o.member_id,
                COALESCE(m.name,''), o.total_cents, o.discount_cents, o.pay_cents, o.status,
                (SELECT COUNT(*) FROM sales_order_item i WHERE i.order_no=o.order_no),
                COALESCE((SELECT SUM(r.amount_cents) FROM refund_record r WHERE r.order_no=o.order_no),0)
         FROM sales_order o
         LEFT JOIN member m ON m.member_id=o.member_id
         WHERE (?1 IS NULL OR o.business_date=?1)
           AND (?2 = '' OR o.order_no LIKE ?3)
         ORDER BY o.created_at DESC LIMIT ?4",
    )?;
    let rows = stmt.query_map(params![date, keyword, pattern, limit], |r| {
        let member_name: String = r.get(5)?;
        Ok(OrderSummary {
            order_no: r.get(0)?,
            business_date: r.get(1)?,
            created_at: r.get(2)?,
            cashier_id: r.get(3)?,
            member_id: r.get(4)?,
            member_name: if member_name.is_empty() {
                None
            } else {
                Some(member_name)
            },
            total_cents: r.get(6)?,
            discount_cents: r.get(7)?,
            pay_cents: r.get(8)?,
            status: r.get(9)?,
            item_count: r.get(10)?,
            refunded_cents: r.get(11)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// 订单详情：头 + 明细 + 支付流水 + 优惠 + 退货记录。
/// 订单不存在返回 None。
pub fn order_detail(conn: &Connection, order_no: &str) -> Result<Option<OrderDetail>> {
    let header = conn
        .query_row(
            "SELECT o.order_no, o.business_date, o.created_at, o.cashier_id, o.member_id,
                    COALESCE(m.name,''), o.total_cents, o.discount_cents, o.pay_cents, o.status,
                    (SELECT COUNT(*) FROM sales_order_item i WHERE i.order_no=o.order_no),
                    COALESCE((SELECT SUM(r.amount_cents) FROM refund_record r WHERE r.order_no=o.order_no),0)
             FROM sales_order o
             LEFT JOIN member m ON m.member_id=o.member_id
             WHERE o.order_no=?1",
            [order_no],
            |r| {
                let member_name: String = r.get(5)?;
                Ok(OrderSummary {
                    order_no: r.get(0)?,
                    business_date: r.get(1)?,
                    created_at: r.get(2)?,
                    cashier_id: r.get(3)?,
                    member_id: r.get(4)?,
                    member_name: if member_name.is_empty() {
                        None
                    } else {
                        Some(member_name)
                    },
                    total_cents: r.get(6)?,
                    discount_cents: r.get(7)?,
                    pay_cents: r.get(8)?,
                    status: r.get(9)?,
                    item_count: r.get(10)?,
                    refunded_cents: r.get(11)?,
                })
            },
        )
        .optional()?;

    let mut items = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT i.item_id, i.sku_id, COALESCE(p.spu_name,''), i.qty, i.returned_qty,
                    i.unit_price_cents, i.line_total_cents, i.discount_cents, i.is_gift
             FROM sales_order_item i
             LEFT JOIN sku s ON s.sku_id=i.sku_id
             LEFT JOIN spu p ON p.spu_id=s.spu_id
             WHERE i.order_no=?1 ORDER BY i.item_id",
        )?;
        let rows = stmt.query_map([order_no], |r| {
            Ok(OrderItemDetail {
                item_id: r.get(0)?,
                sku_id: r.get(1)?,
                product_name: r.get(2)?,
                qty: r.get(3)?,
                returned_qty: r.get(4)?,
                unit_price_cents: r.get(5)?,
                line_total_cents: r.get(6)?,
                discount_cents: r.get(7)?,
                is_gift: r.get::<_, i64>(8)? != 0,
            })
        })?;
        for row in rows {
            items.push(row?);
        }
    }

    let mut payments = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT pay_id, pay_channel, pay_mode, amount_cents, cash_tendered_cents,
                    change_cents, out_trade_no, trade_no, status, paid_at, created_at
             FROM payment_record WHERE order_no=?1 ORDER BY pay_id",
        )?;
        let rows = stmt.query_map([order_no], |r| {
            Ok(PaymentDetail {
                pay_id: r.get(0)?,
                pay_channel: r.get(1)?,
                pay_mode: r.get(2)?,
                amount_cents: r.get(3)?,
                cash_tendered_cents: r.get(4)?,
                change_cents: r.get(5)?,
                out_trade_no: r.get(6)?,
                trade_no: r.get(7)?,
                status: r.get(8)?,
                paid_at: r.get(9)?,
                created_at: r.get(10)?,
            })
        })?;
        for row in rows {
            payments.push(row?);
        }
    }

    let mut promos = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT promotion_id, discount_cents, detail_json FROM order_promotion
             WHERE order_no=?1 ORDER BY rowid",
        )?;
        let rows = stmt.query_map([order_no], |r| {
            Ok(PromoDetail {
                promotion_id: r.get(0)?,
                discount_cents: r.get(1)?,
                detail: r.get(2)?,
            })
        })?;
        for row in rows {
            promos.push(row?);
        }
    }

    let mut refunds = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT refund_id, return_no, refund_no, amount_cents, status, created_at
             FROM refund_record WHERE order_no=?1 ORDER BY refund_id",
        )?;
        let rows = stmt.query_map([order_no], |r| {
            Ok(RefundDetail {
                refund_id: r.get(0)?,
                return_no: r.get(1)?,
                refund_no: r.get(2)?,
                amount_cents: r.get(3)?,
                status: r.get(4)?,
                created_at: r.get(5)?,
            })
        })?;
        for row in rows {
            refunds.push(row?);
        }
    }

    // 订单不存在：直接返回 None，其余明细一并丢弃
    let Some(header) = header else {
        return Ok(None);
    };

    Ok(Some(OrderDetail {
        header,
        items,
        payments,
        promos,
        refunds,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{open_in_memory, run_migrations};

    fn setup() -> Connection {
        let mut c = open_in_memory().unwrap();
        run_migrations(&mut c).unwrap();
        c.execute(
            "INSERT INTO spu(category_id,spu_name,spu_code) VALUES (1,'测试商品','S1')",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO sku(spu_id,barcode,sale_price_cents) VALUES (1,'B1',1000)",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO sales_order(order_no,device_id,cashier_id,shift_id,total_cents,discount_cents,pay_cents,status,business_date,sync_status,created_at)
             VALUES ('O1','POS1',1,1,2000,0,2000,'COMPLETED','2026-11-18','LOCAL_ONLY','2026-11-18T01:00:00Z')",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO sales_order_item(order_no,sku_id,qty,unit_price_cents,line_total_cents)
             VALUES ('O1',1,2,1000,2000)",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO payment_record(order_no,pay_channel,amount_cents,out_trade_no,status,created_at)
             VALUES ('O1','CASH',2000,'OT1','SUCCESS','2026-11-18T01:00:01Z')",
            [],
        )
        .unwrap();
        c
    }

    #[test]
    fn list_orders_filters_by_date_and_keyword() {
        let c = setup();
        assert_eq!(
            list_orders(&c, Some("2026-11-18"), "", 10).unwrap().len(),
            1
        );
        assert_eq!(
            list_orders(&c, Some("2026-11-17"), "", 10).unwrap().len(),
            0
        );
        assert_eq!(list_orders(&c, None, "O1", 10).unwrap().len(), 1);
        assert_eq!(list_orders(&c, None, "ZZ", 10).unwrap().len(), 0);
    }

    #[test]
    fn detail_contains_items_and_payments() {
        let c = setup();
        let d = order_detail(&c, "O1").unwrap().unwrap();
        assert_eq!(d.header.order_no, "O1");
        assert_eq!(d.items.len(), 1);
        assert_eq!(d.items[0].product_name, "测试商品");
        assert_eq!(d.payments.len(), 1);
        assert_eq!(d.payments[0].pay_channel, "CASH");
        assert!(order_detail(&c, "NOPE").unwrap().is_none());
    }
}
