//! 会员四账（§13）：PRINCIPAL/BONUS/POINTS/GROWTH。
//! 余额不可直接 UPDATE，必须写不可变流水；扣款先 BONUS 后 PRINCIPAL；
//! 退款按原单构成镜像回补，防赠送金洗成可提现本金。

use crate::error::{CoreError, Result};
use crate::time::format_utc;
use chrono::Utc;
use rusqlite::{params, Connection};

pub const PRINCIPAL: &str = "PRINCIPAL";
pub const BONUS: &str = "BONUS";
pub const POINTS: &str = "POINTS";
pub const GROWTH: &str = "GROWTH";

/// 写一条资产流水（不可变、幂等）。返回是否真的写入（重复幂等键返回 false）。
/// 不自行开启事务：调用方负责把同一业务的多条流水放在一个事务里，
/// 否则"扣赠送成功、扣本金失败"会留下半扣状态。
#[allow(clippy::too_many_arguments)]
pub fn append_asset_log(
    conn: &Connection,
    member_id: i64,
    asset_type: &str,
    change: i64,
    reason: &str,
    ref_order_no: Option<&str>,
    idempotency_key: &str,
    now: &str,
) -> Result<bool> {
    // 幂等：唯一键冲突返回首次结果，不重复变动
    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM member_asset_log WHERE idempotency_key=?1",
        [idempotency_key],
        |r| r.get(0),
    )?;
    if exists > 0 {
        return Ok(false);
    }
    let current: i64 = conn
        .query_row(
            "SELECT balance FROM member_asset WHERE member_id=?1 AND asset_type=?2",
            params![member_id, asset_type],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let new_balance = current + change;
    // 注意：透支管控在 deduct/refund_mirror 等高层操作中执行；
    // append_asset_log 是不可变流水投影原语，只负责幂等记账。
    conn.execute(
        "INSERT INTO member_asset_log(member_id,asset_type,change_amount,balance_after,reason,ref_order_no,idempotency_key,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        params![member_id, asset_type, change, new_balance, reason, ref_order_no, idempotency_key, now],
    )?;
    conn.execute(
        "INSERT INTO member_asset(member_id,asset_type,balance,version) VALUES (?1,?2,?3,1)
         ON CONFLICT(member_id,asset_type) DO UPDATE SET balance=excluded.balance, version=member_asset.version+1",
        params![member_id, asset_type, new_balance],
    )?;
    Ok(true)
}

/// 充值入账（§13.3）：本金 += 实充，赠送 += 赠送额，不计成长值。
pub fn recharge(
    conn: &Connection,
    member_id: i64,
    recharge_no: &str,
    principal_cents: i64,
    bonus_cents: i64,
) -> Result<()> {
    let now = format_utc(&Utc::now());
    append_asset_log(
        conn,
        member_id,
        PRINCIPAL,
        principal_cents,
        "RECHARGE",
        None,
        &format!("RECHARGE:{recharge_no}:{PRINCIPAL}"),
        &now,
    )?;
    if bonus_cents > 0 {
        append_asset_log(
            conn,
            member_id,
            BONUS,
            bonus_cents,
            "RECHARGE_BONUS",
            None,
            &format!("RECHARGE:{recharge_no}:{BONUS}"),
            &now,
        )?;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Deduction {
    pub bonus_cents: i64,
    pub principal_cents: i64,
}

/// 储值扣款（§13.2 硬编码顺序）：先 BONUS，不足部分扣 PRINCIPAL。
pub fn deduct(
    conn: &Connection,
    member_id: i64,
    order_no: &str,
    amount_cents: i64,
) -> Result<Deduction> {
    let bonus_bal: i64 = conn
        .query_row(
            "SELECT balance FROM member_asset WHERE member_id=?1 AND asset_type=?2",
            params![member_id, BONUS],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let principal_bal: i64 = conn
        .query_row(
            "SELECT balance FROM member_asset WHERE member_id=?1 AND asset_type=?2",
            params![member_id, PRINCIPAL],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let bonus_use = amount_cents.min(bonus_bal);
    let principal_use = amount_cents - bonus_use;
    if principal_use > principal_bal {
        return Err(CoreError::Validation("储值余额不足".into()));
    }
    let now = format_utc(&Utc::now());
    if bonus_use > 0 {
        append_asset_log(
            conn,
            member_id,
            BONUS,
            -bonus_use,
            "SALE",
            Some(order_no),
            &format!("SALE:{order_no}:{BONUS}"),
            &now,
        )?;
    }
    if principal_use > 0 {
        append_asset_log(
            conn,
            member_id,
            PRINCIPAL,
            -principal_use,
            "SALE",
            Some(order_no),
            &format!("SALE:{order_no}:{PRINCIPAL}"),
            &now,
        )?;
    }
    Ok(Deduction {
        bonus_cents: bonus_use,
        principal_cents: principal_use,
    })
}

/// 退款镜像回补（§13.2）：先补 BONUS（上限=原单 BONUS 扣减），超出补 PRINCIPAL
/// （上限=原单 PRINCIPAL 扣减）。超出总额 → 程序错误，拒绝执行。
pub fn refund_mirror(
    conn: &Connection,
    member_id: i64,
    refund_no: &str,
    order_no: &str,
    refund_amount: i64,
    original: Deduction,
) -> Result<Deduction> {
    let total_original = original.bonus_cents + original.principal_cents;
    if refund_amount > total_original {
        return Err(CoreError::Validation(
            "退款额超出原单储值扣减总额，拒绝执行".into(),
        ));
    }
    // 累计上限：同一订单多次部分退货时，回补总额不得超过原扣减构成。
    let refunded_bonus: i64 = conn.query_row(
        "SELECT COALESCE(SUM(change_amount),0) FROM member_asset_log
         WHERE member_id=?1 AND asset_type=?2 AND reason='REFUND' AND ref_order_no=?3",
        params![member_id, BONUS, order_no],
        |r| r.get(0),
    )?;
    let refunded_principal: i64 = conn.query_row(
        "SELECT COALESCE(SUM(change_amount),0) FROM member_asset_log
         WHERE member_id=?1 AND asset_type=?2 AND reason='REFUND' AND ref_order_no=?3",
        params![member_id, PRINCIPAL, order_no],
        |r| r.get(0),
    )?;
    let bonus_left = (original.bonus_cents - refunded_bonus).max(0);
    let principal_left = (original.principal_cents - refunded_principal).max(0);
    let bonus_back = refund_amount.min(bonus_left);
    let principal_back = (refund_amount - bonus_back).min(principal_left);
    if bonus_back + principal_back != refund_amount {
        return Err(CoreError::Validation(format!(
            "累计退款超出原单剩余可回补额度（剩余赠送 {bonus_left}，剩余本金 {principal_left}）"
        )));
    }
    let now = format_utc(&Utc::now());
    if bonus_back > 0 {
        append_asset_log(
            conn,
            member_id,
            BONUS,
            bonus_back,
            "REFUND",
            Some(order_no),
            &format!("REFUND:{refund_no}:{BONUS}"),
            &now,
        )?;
    }
    if principal_back > 0 {
        append_asset_log(
            conn,
            member_id,
            PRINCIPAL,
            principal_back,
            "REFUND",
            Some(order_no),
            &format!("REFUND:{refund_no}:{PRINCIPAL}"),
            &now,
        )?;
    }
    Ok(Deduction {
        bonus_cents: bonus_back,
        principal_cents: principal_back,
    })
}

/// 积分累积（§13.4）：每消费 1 元积 N 分（N 由系统配置「每元赠送积分」决定），
/// BONUS 赠送金抵扣部分不计。返回本次实际赠送的积分数，供结算界面回显。
pub fn accrue_points(
    conn: &Connection,
    member_id: i64,
    order_no: &str,
    payable_excluding_bonus: i64,
    points_per_yuan: i64,
) -> Result<i64> {
    let points = payable_excluding_bonus / 100 * points_per_yuan;
    if points <= 0 {
        return Ok(0);
    }
    append_asset_log(
        conn,
        member_id,
        POINTS,
        points,
        "SALE_POINTS",
        Some(order_no),
        &format!("SALE:{order_no}:{POINTS}"),
        &format_utc(&Utc::now()),
    )?;
    Ok(points)
}

/// 券状态机（§13.7）：拒绝非法转移 REFUNDED → REDEEMED。
pub fn transition_coupon(conn: &Connection, coupon_code: &str, to: &str) -> Result<()> {
    let current: String = conn.query_row(
        "SELECT status FROM coupon WHERE coupon_code=?1",
        [coupon_code],
        |r| r.get(0),
    )?;
    let legal = matches!(
        (current.as_str(), to),
        ("UNUSED", "LOCKED")
            | ("LOCKED", "REDEEMED")
            | ("LOCKED", "UNUSED")
            | ("REDEEMED", "REFUNDED")
            | ("UNUSED", "EXPIRED")
            | ("LOCKED", "EXPIRED")
    );
    if !legal {
        return Err(CoreError::Validation(format!(
            "非法券状态转移: {current} → {to}"
        )));
    }
    conn.execute(
        "UPDATE coupon SET status=?1 WHERE coupon_code=?2",
        params![to, coupon_code],
    )?;
    Ok(())
}

/// 券懒更新过期（读路径双保险之一）。
pub fn expire_lazy(conn: &Connection, coupon_code: &str, now: &str) -> Result<()> {
    conn.execute(
        "UPDATE coupon SET status='EXPIRED'
         WHERE coupon_code=?1 AND status='UNUSED' AND valid_until IS NOT NULL AND valid_until < ?2",
        params![coupon_code, now],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{open_in_memory, run_migrations};
    use serde_json::json;

    fn setup() -> Connection {
        let mut c = open_in_memory().unwrap();
        run_migrations(&mut c).unwrap();
        c.execute(
            "INSERT INTO member(phone, member_code) VALUES ('13800000000','M1')",
            [],
        )
        .unwrap();
        c
    }

    #[test]
    fn recharge_then_deduct_bonus_first() {
        let c = setup();
        recharge(&c, 1, "R1", 10000, 2000).unwrap();
        let d = deduct(&c, 1, "ORD1", 1500).unwrap();
        assert_eq!(
            d,
            Deduction {
                bonus_cents: 1500,
                principal_cents: 0
            }
        );
        let bonus: i64 = c
            .query_row(
                "SELECT balance FROM member_asset WHERE member_id=1 AND asset_type='BONUS'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(bonus, 500);
    }

    #[test]
    fn deduct_overflow_to_principal() {
        let c = setup();
        recharge(&c, 1, "R1", 10000, 2000).unwrap();
        let d = deduct(&c, 1, "ORD1", 5000).unwrap();
        assert_eq!(
            d,
            Deduction {
                bonus_cents: 2000,
                principal_cents: 3000
            }
        );
    }

    #[test]
    fn mirror_refund_prevents_money_laundering() {
        let c = setup();
        // 充值 100 赠 20，用 BONUS 消费 20，退 10
        recharge(&c, 1, "R1", 10000, 2000).unwrap();
        let d = deduct(&c, 1, "ORD1", 2000).unwrap();
        assert_eq!(d.bonus_cents, 2000);
        refund_mirror(&c, 1, "RF1", "ORD1", 1000, d).unwrap();
        let bonus: i64 = c
            .query_row(
                "SELECT balance FROM member_asset WHERE member_id=1 AND asset_type='BONUS'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let principal: i64 = c
            .query_row(
                "SELECT balance FROM member_asset WHERE member_id=1 AND asset_type='PRINCIPAL'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(bonus, 1000);
        assert_eq!(principal, 10000, "本金不得虚增");
    }

    #[test]
    fn mirror_refund_rejects_over_refund() {
        let c = setup();
        recharge(&c, 1, "R1", 10000, 2000).unwrap();
        let d = deduct(&c, 1, "ORD1", 2000).unwrap();
        assert!(refund_mirror(&c, 1, "RF1", "ORD1", 3000, d).is_err());
    }

    #[test]
    fn mirror_refund_cumulative_cap() {
        let c = setup();
        recharge(&c, 1, "R1", 10000, 2000).unwrap();
        let d = deduct(&c, 1, "ORD1", 2000).unwrap();
        refund_mirror(&c, 1, "RF1", "ORD1", 1500, d).unwrap();
        // 剩余可回补仅 500，再次按 1500 部分退货必须拒绝
        assert!(refund_mirror(&c, 1, "RF2", "ORD1", 1500, d).is_err());
    }

    #[test]
    fn idempotency_prevents_double_deduct() {
        let c = setup();
        let now = format_utc(&Utc::now());
        assert!(append_asset_log(
            &c,
            1,
            PRINCIPAL,
            -100,
            "SALE",
            Some("O1"),
            "SALE:O1:PRINCIPAL",
            &now
        )
        .unwrap());
        assert!(!append_asset_log(
            &c,
            1,
            PRINCIPAL,
            -100,
            "SALE",
            Some("O1"),
            "SALE:O1:PRINCIPAL",
            &now
        )
        .unwrap());
        let bal: i64 = c
            .query_row(
                "SELECT balance FROM member_asset WHERE member_id=1 AND asset_type='PRINCIPAL'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(bal, -100);
    }

    #[test]
    fn coupon_illegal_transition_rejected() {
        let c = setup();
        c.execute(
            "INSERT INTO coupon(coupon_code, discount_cents) VALUES ('C1', 1000)",
            [],
        )
        .unwrap();
        transition_coupon(&c, "C1", "LOCKED").unwrap();
        transition_coupon(&c, "C1", "REDEEMED").unwrap();
        transition_coupon(&c, "C1", "REFUNDED").unwrap();
        assert!(
            transition_coupon(&c, "C1", "REDEEMED").is_err(),
            "REFUNDED 为终态"
        );
    }

    #[test]
    fn lazy_expire() {
        let c = setup();
        c.execute(
            "INSERT INTO coupon(coupon_code, discount_cents, valid_until) VALUES ('C2', 1000, '2026-01-01T00:00:00Z')",
            [],
        ).unwrap();
        expire_lazy(&c, "C2", "2026-11-17T00:00:00Z").unwrap();
        let s: String = c
            .query_row(
                "SELECT status FROM coupon WHERE coupon_code='C2'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(s, "EXPIRED");
    }

    #[test]
    fn points_allow_negative() {
        let c = setup();
        append_asset_log(
            &c,
            1,
            POINTS,
            10,
            "EARN",
            None,
            "E:1",
            "2026-01-01T00:00:00Z",
        )
        .unwrap();
        append_asset_log(
            &c,
            1,
            POINTS,
            -20,
            "REFUND",
            None,
            "E:2",
            "2026-01-01T00:00:00Z",
        )
        .unwrap();
        let bal: i64 = c
            .query_row(
                "SELECT balance FROM member_asset WHERE member_id=1 AND asset_type='POINTS'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(bal, -10);
    }

    #[test]
    fn accrue_points_returns_earned_and_skips_zero() {
        let c = setup();
        // 应付 2500 分 = 25 元，每元 2 分 → 50 分
        let earned = accrue_points(&c, 1, "ORD-P", 2500, 2).unwrap();
        assert_eq!(earned, 50);
        let bal: i64 = c
            .query_row(
                "SELECT balance FROM member_asset WHERE member_id=1 AND asset_type='POINTS'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(bal, 50);
        // 不足一元或每元 0 分 → 不写流水，返回 0
        assert_eq!(accrue_points(&c, 1, "ORD-Z", 99, 2).unwrap(), 0);
        assert_eq!(accrue_points(&c, 1, "ORD-R", 1000, 0).unwrap(), 0);
    }

    #[test]
    fn json_detail_compiles() {
        let _ = json!({"refund": 1});
    }
}
