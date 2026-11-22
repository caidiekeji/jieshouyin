//! 促销引擎（§8）。计算顺序：单品级 → 整单级 → 券核销。
//! 分摊采用剩余应付金额比例 + 最大余数法（§8.4）。
//! 会员价与单品促销同一行互斥取优：分别按「仅会员价」「仅促销」计算完整行优惠，
//! 逐行选其一，绝不把促销叠加在已折的会员价上；每笔促销保存逐行分摊明细，
//! 触发叠加上限时只在原优惠行内缩放。

use crate::error::{CoreError, Result};
use crate::money::allocate_by_remaining;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CartLine {
    pub sku_id: i64,
    pub qty: i64,
    pub unit_price_cents: i64,
    pub line_total_cents: i64,
    pub discount_cents: i64,
    pub member_price_cents: Option<i64>,
    pub is_gift: bool,
    pub applied_promos: Vec<i64>,
    /// 养生服务行指定的技师；零售与餐饮为空。
    #[serde(default)]
    pub technician_id: Option<i64>,
}

impl CartLine {
    pub fn new(sku_id: i64, qty: i64, unit_price_cents: i64) -> Result<Self> {
        if qty <= 0 || unit_price_cents < 0 {
            return Err(CoreError::Validation("数量或单价非法".into()));
        }
        let line_total = qty
            .checked_mul(unit_price_cents)
            .ok_or(CoreError::Money("行金额溢出".into()))?;
        Ok(Self {
            sku_id,
            qty,
            unit_price_cents,
            line_total_cents: line_total,
            discount_cents: 0,
            member_price_cents: None,
            is_gift: false,
            applied_promos: vec![],
            technician_id: None,
        })
    }

    pub fn payable(&self) -> i64 {
        self.line_total_cents - self.discount_cents
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[allow(non_camel_case_types)] // 状态/类型字符串与文档一致
pub enum Scope {
    SINGLE_ITEM,
    ORDER_LEVEL,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Promotion {
    pub promotion_id: i64,
    pub scope: Scope,
    pub priority: i32,
    pub exclusive: bool,
    pub stackable: bool,
    /// 门槛：满分件数 / 满金额（分）
    pub min_qty: i64,
    pub min_amount_cents: i64,
    /// 动作：折扣率（0.85=85折）或满减额（分）
    pub discount_rate: Option<f64>,
    pub discount_cents: Option<i64>,
    pub target_skus: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppliedPromo {
    pub promotion_id: i64,
    pub discount_cents: i64,
    pub detail: String,
    /// 逐行分摊明细（行下标, 分摊金额）。促销裁剪只允许在这些原优惠行内按比例缩放，
    /// 不得重新向全单分摊——否则非活动商品也会被摊到优惠。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allocations: Vec<(usize, i64)>,
}

impl AppliedPromo {
    fn single(promotion_id: i64, allocations: Vec<(usize, i64)>) -> Self {
        let discount_cents = allocations.iter().map(|(_, a)| *a).sum();
        Self {
            promotion_id,
            discount_cents,
            detail: "SINGLE_ITEM".into(),
            allocations,
        }
    }

    fn order(promotion_id: i64, allocations: Vec<(usize, i64)>) -> Self {
        let discount_cents = allocations.iter().map(|(_, a)| *a).sum();
        Self {
            promotion_id,
            discount_cents,
            detail: "ORDER_LEVEL".into(),
            allocations,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromoResult {
    pub applied: Vec<AppliedPromo>,
    pub total_discount_cents: i64,
    pub skipped: Vec<String>,
}

/// 计算促销。会员价与单品促销互斥取优；整单级以单品优惠后金额为门槛。
pub fn calc_promotions(
    lines: &mut [CartLine],
    member_level: Option<i32>,
    promos: &[Promotion],
    max_discount_ratio: f64,
) -> Result<PromoResult> {
    let mut applied: Vec<AppliedPromo> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    let original_total: i64 = lines
        .iter()
        .filter(|l| !l.is_gift)
        .map(|l| l.line_total_cents)
        .sum();

    let n = lines.len();
    // 会员价优惠按行独立计算（不是促销，不参与「叠加上限」）。
    let mut member_amounts: Vec<i64> = vec![0; n];
    if member_level.is_some() {
        for (i, line) in lines.iter().enumerate() {
            if line.is_gift || line.line_total_cents == 0 {
                continue;
            }
            if let Some(mp) = line.member_price_cents {
                if mp >= 0 && mp < line.unit_price_cents {
                    let md = ((line.unit_price_cents - mp) * line.qty).min(line.line_total_cents);
                    if md > 0 {
                        member_amounts[i] = md;
                    }
                }
            }
        }
    }
    // 逐行状态：会员价是否生效、单品促销已占金额、整单促销已占金额。
    // 一行要么享受会员价，要么享受单品促销，二者不叠加。
    let mut member_active: Vec<bool> = member_amounts.iter().map(|&m| m > 0).collect();
    let mut single_used: Vec<i64> = vec![0; n];
    let mut order_used: Vec<i64> = vec![0; n];

    // 第 1 层：单品级。促销基数一律取原行金额（会员价生效时用于「取优」比较），
    // 只有该行已落促销后才按剩余应付继续计算（可叠加促销）。
    let mut singles: Vec<&Promotion> = promos
        .iter()
        .filter(|p| p.scope == Scope::SINGLE_ITEM)
        .collect();
    singles.sort_by_key(|p| p.priority);

    for promo in singles {
        let mut candidates: Vec<(usize, i64)> = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            if line.is_gift || line.line_total_cents == 0 {
                continue;
            }
            if !promo.target_skus.is_empty() && !promo.target_skus.contains(&line.sku_id) {
                continue;
            }
            if line.qty < promo.min_qty || line.line_total_cents < promo.min_amount_cents {
                continue;
            }
            let base = if member_active[i] {
                line.line_total_cents
            } else {
                line.line_total_cents - single_used[i]
            };
            if base <= 0 {
                continue;
            }
            let promo_discount = promo_discount_for(base, promo)?.min(base);
            if promo_discount <= 0 {
                continue;
            }
            // 会员价与单品促销同一行取优：会员优惠更大时不叠加促销。
            if member_active[i] && member_amounts[i] >= promo_discount {
                skipped.push(format!(
                    "SKU {} 会员价优于单品促销 {}",
                    line.sku_id, promo.promotion_id
                ));
                continue;
            }
            candidates.push((i, promo_discount));
        }
        if candidates.is_empty() {
            continue;
        }
        if promo.discount_cents.is_some() {
            // 单品满减：只对优惠额最大的一行减一次，避免"买 N 行减 N 次"。
            if let Some((idx, amount)) = candidates.iter().max_by_key(|(_, d)| *d).copied() {
                applied.push(AppliedPromo::single(
                    promo.promotion_id,
                    vec![(idx, amount)],
                ));
                single_used[idx] += amount;
                member_active[idx] = false;
            }
        } else {
            // 折扣率：逐行计算，且只落目标行。
            let mut allocations = Vec::with_capacity(candidates.len());
            for (i, amount) in candidates {
                single_used[i] += amount;
                member_active[i] = false;
                allocations.push((i, amount));
            }
            applied.push(AppliedPromo::single(promo.promotion_id, allocations));
        }
        // 互斥/不可叠加：本层此后再无促销生效
        if promo.exclusive || !promo.stackable {
            break;
        }
    }

    // 第 2 层：整单级（门槛 = 单品优惠后累计额，按剩余应付比例分摊到全部非赠品行）
    let mut orders: Vec<&Promotion> = promos
        .iter()
        .filter(|p| p.scope == Scope::ORDER_LEVEL)
        .collect();
    orders.sort_by_key(|p| p.priority);
    let mut layer_locked = false;
    for promo in orders {
        if layer_locked {
            break;
        }
        let base: i64 = lines
            .iter()
            .enumerate()
            .filter(|(_, l)| !l.is_gift)
            .map(|(i, l)| {
                remaining_of(
                    i,
                    &member_active,
                    &member_amounts,
                    &single_used,
                    &order_used,
                    l,
                )
            })
            .map(|d| 0i64.max(d))
            .sum();
        let qty: i64 = lines.iter().filter(|l| !l.is_gift).map(|l| l.qty).sum();
        if qty < promo.min_qty || base < promo.min_amount_cents {
            skipped.push(format!("整单促销 {} 未达门槛", promo.promotion_id));
            continue;
        }
        let amount = promo_discount_for(base, promo)?.min(base);
        if amount <= 0 {
            continue;
        }
        let indexes: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, l)| !l.is_gift)
            .map(|(i, _)| i)
            .collect();
        let remaining: Vec<i64> = indexes
            .iter()
            .map(|&i| {
                remaining_of(
                    i,
                    &member_active,
                    &member_amounts,
                    &single_used,
                    &order_used,
                    &lines[i],
                )
            })
            .collect();
        if remaining.iter().all(|&r| r <= 0) {
            continue;
        }
        let alloc = allocate_by_remaining(&remaining, amount)?;
        let mut allocations = Vec::new();
        for (pos, a) in alloc.into_iter().enumerate() {
            if a > 0 {
                order_used[indexes[pos]] += a;
                allocations.push((indexes[pos], a));
            }
        }
        applied.push(AppliedPromo::order(promo.promotion_id, allocations));
        if promo.exclusive || !promo.stackable {
            layer_locked = true;
        }
    }

    // 第 3 层：券核销（门槛 = 应付额）
    let total_discount: i64 = applied.iter().map(|a| a.discount_cents).sum();
    // 叠加上限：优惠 ≤ (1 - max_discount_ratio) * 原价。
    // 用整数基点（bps）计算避免浮点误差（如 1.0-0.8=0.19999... 导致多削 1 分）。
    let keep_bps = ((max_discount_ratio * 10000.0).round() as i64).clamp(0, 10000);
    let max_allowed = original_total * (10000 - keep_bps) / 10000;
    if total_discount > max_allowed {
        let mut remaining_excess = total_discount - max_allowed;
        // 逆序削减：后应用的先削；每笔促销只在自己的原优惠行内按比例缩放。
        for a in applied.iter_mut().rev() {
            if remaining_excess <= 0 {
                break;
            }
            let cut = a.discount_cents.min(remaining_excess);
            let keep = a.discount_cents - cut;
            a.allocations = scale_allocations(&a.allocations, keep)?;
            a.discount_cents = keep;
            remaining_excess -= cut;
            skipped.push(format!("促销 {} 因叠加上限被削减 {cut} 分", a.promotion_id));
        }
        single_used = vec![0; n];
        order_used = vec![0; n];
        for a in &applied {
            for (idx, amount) in &a.allocations {
                if a.detail == "ORDER_LEVEL" {
                    order_used[*idx] += *amount;
                } else {
                    single_used[*idx] += *amount;
                }
            }
        }
    }

    // 落地：行优惠 = 会员价（或单品促销）+ 整单促销，逐行不穿透行金额。
    for (i, line) in lines.iter_mut().enumerate() {
        let member_part = if member_active[i] {
            member_amounts[i]
        } else {
            0
        };
        let discount = member_part + single_used[i] + order_used[i];
        if discount > line.line_total_cents {
            return Err(CoreError::Money(format!(
                "行 {} 优惠 {discount} 穿透行金额 {}",
                line.sku_id, line.line_total_cents
            )));
        }
        line.discount_cents = discount;
        line.applied_promos.clear();
        for a in &applied {
            if a.allocations.iter().any(|(idx, _)| *idx == i)
                && !line.applied_promos.contains(&a.promotion_id)
            {
                line.applied_promos.push(a.promotion_id);
            }
        }
    }

    let total_discount_cents: i64 = applied.iter().map(|a| a.discount_cents).sum();
    let member_total: i64 = lines
        .iter()
        .enumerate()
        .map(|(i, _)| {
            if member_active[i] {
                member_amounts[i]
            } else {
                0
            }
        })
        .sum();
    // 最终不变式校验：行优惠 = 会员价优惠 + 促销优惠
    let line_sum: i64 = lines.iter().map(|l| l.discount_cents).sum();
    if line_sum != total_discount_cents + member_total {
        return Err(CoreError::Money(format!(
            "行优惠合计 {line_sum} != 会员优惠 {member_total} + 促销合计 {total_discount_cents}"
        )));
    }
    Ok(PromoResult {
        applied,
        total_discount_cents,
        skipped,
    })
}

/// 当前行的剩余应付金额（含会员价、单品促销与整单促销占用）。
fn remaining_of(
    i: usize,
    member_active: &[bool],
    member_amounts: &[i64],
    single_used: &[i64],
    order_used: &[i64],
    line: &CartLine,
) -> i64 {
    let member_part = if member_active[i] {
        member_amounts[i]
    } else {
        0
    };
    line.line_total_cents - (member_part + single_used[i] + order_used[i])
}

fn promo_discount_for(base: i64, promo: &Promotion) -> Result<i64> {
    if let Some(cents) = promo.discount_cents {
        return Ok(cents.min(base));
    }
    if let Some(rate) = promo.discount_rate {
        if !(0.0..=1.0).contains(&rate) {
            return Err(CoreError::Validation("折扣率须在 0~1 之间".into()));
        }
        return Ok(((base as f64) * (1.0 - rate)) as i64);
    }
    Ok(0)
}

/// 裁剪后重算分摊：只在原优惠行内按原比例缩放，绝不向全单重新分摊。
fn scale_allocations(allocations: &[(usize, i64)], keep: i64) -> Result<Vec<(usize, i64)>> {
    if keep <= 0 || allocations.is_empty() {
        return Ok(vec![]);
    }
    let weights: Vec<i64> = allocations.iter().map(|(_, a)| *a).collect();
    let from: i64 = weights.iter().sum();
    if from <= 0 {
        return Ok(vec![]);
    }
    if keep >= from {
        return Ok(allocations.to_vec());
    }
    let scaled = allocate_by_remaining(&weights, keep)?;
    Ok(allocations
        .iter()
        .zip(scaled)
        .map(|((idx, _), amount)| (*idx, amount))
        .filter(|(_, amount)| *amount > 0)
        .collect())
}

/// 退单还原（§8.6）：按原单行优惠还原，不重查规则。
/// 行内部分退：退款额 = (line_total − discount) × return_qty / line_qty（整除），
/// 最后一次退清时退剩余全部。
pub fn refund_for_line(line: &CartLine, return_qty: i64, previously_returned: i64) -> Result<i64> {
    let payable = line.payable();
    let remaining_qty = line.qty - previously_returned;
    if return_qty <= 0 || return_qty > remaining_qty {
        return Err(CoreError::Validation("退货数量超出可退数量".into()));
    }
    if return_qty == remaining_qty {
        // 最后一次退清，退剩余全部
        return Ok(payable - (payable * previously_returned / line.qty));
    }
    Ok(payable * return_qty / line.qty)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn promo(id: i64, scope: Scope, amount: i64, exclusive: bool) -> Promotion {
        Promotion {
            promotion_id: id,
            scope,
            priority: 100 + id as i32,
            exclusive,
            stackable: !exclusive,
            min_qty: 0,
            min_amount_cents: 0,
            discount_rate: None,
            discount_cents: Some(amount),
            target_skus: vec![],
        }
    }

    fn rate_promo(id: i64, scope: Scope, rate: f64) -> Promotion {
        Promotion {
            promotion_id: id,
            scope,
            priority: 100 + id as i32,
            exclusive: false,
            stackable: true,
            min_qty: 0,
            min_amount_cents: 0,
            discount_rate: Some(rate),
            discount_cents: None,
            target_skus: vec![],
        }
    }

    #[test]
    fn single_promo_applies() {
        let mut lines = vec![CartLine::new(1, 2, 5000).unwrap()];
        let r = calc_promotions(
            &mut lines,
            None,
            &[promo(1, Scope::SINGLE_ITEM, 1000, false)],
            0.8,
        )
        .unwrap();
        assert_eq!(r.total_discount_cents, 1000);
        assert_eq!(lines[0].discount_cents, 1000);
        assert_eq!(lines[0].payable(), 9000);
    }

    #[test]
    fn stacked_allocations_sum_exact() {
        let mut lines = vec![
            CartLine::new(1, 1, 3333).unwrap(),
            CartLine::new(2, 1, 3333).unwrap(),
            CartLine::new(3, 1, 3334).unwrap(),
        ];
        let promos = vec![
            promo(1, Scope::ORDER_LEVEL, 100, false),
            promo(2, Scope::ORDER_LEVEL, 50, false),
        ];
        let r = calc_promotions(&mut lines, None, &promos, 0.8).unwrap();
        assert_eq!(r.total_discount_cents, 150);
        assert_eq!(lines.iter().map(|l| l.discount_cents).sum::<i64>(), 150);
        for l in &lines {
            assert!(l.discount_cents <= l.line_total_cents);
        }
    }

    #[test]
    fn member_price_vs_single_promo_prefer_better() {
        let mut lines = vec![CartLine::new(1, 2, 5000).unwrap()];
        lines[0].member_price_cents = Some(4000); // 优惠 2000
        let r = calc_promotions(
            &mut lines,
            Some(1),
            &[promo(1, Scope::SINGLE_ITEM, 1000, false)],
            0.8,
        )
        .unwrap();
        // 会员价优惠 2000 > 促销 1000：只保留会员价，不叠加促销
        assert_eq!(lines[0].discount_cents, 2000);
        assert_eq!(r.total_discount_cents, 0);
        assert!(r.skipped.iter().any(|s| s.contains("单品促销")));
    }

    #[test]
    fn better_single_promo_replaces_member_price() {
        // 原价 100 元、会员价 80 元、单品五折：取更优的 50 元优惠，应付 50 元。
        // 折扣底线设为 0.5：允许优惠到原价 50%（5000 分），否则会被上限削减到 2000 分，
        // 测不出"促销替换会员价"的意图（叠加上限是另一条独立规则）。
        let mut lines = vec![CartLine::new(1, 1, 10000).unwrap()];
        lines[0].member_price_cents = Some(8000);
        let r = calc_promotions(
            &mut lines,
            Some(1),
            &[rate_promo(1, Scope::SINGLE_ITEM, 0.5)],
            0.5,
        )
        .unwrap();
        assert_eq!(r.total_discount_cents, 5000, "促销应替换会员价，而不是叠加");
        assert_eq!(lines[0].discount_cents, 5000);
        assert_eq!(lines[0].payable(), 5000);
    }

    #[test]
    fn single_promo_only_touches_target_line() {
        // 单品折扣只作用于目标 SKU，非活动商品不得被分摊到优惠。
        let mut lines = vec![
            CartLine::new(1, 1, 2000).unwrap(),
            CartLine::new(2, 1, 8000).unwrap(),
        ];
        let mut p = rate_promo(1, Scope::SINGLE_ITEM, 0.5);
        p.target_skus = vec![1];
        let r = calc_promotions(&mut lines, None, &[p], 0.8).unwrap();
        assert_eq!(lines[0].discount_cents, 1000);
        assert_eq!(lines[1].discount_cents, 0);
        assert_eq!(r.total_discount_cents, 1000);
        assert_eq!(r.applied[0].allocations, vec![(0, 1000)]);
    }

    #[test]
    fn trim_keeps_original_target_line() {
        // 触发上限后，裁剪仍在原优惠行内缩放，不向非活动行重新分摊。
        let mut lines = vec![
            CartLine::new(1, 1, 2000).unwrap(),
            CartLine::new(2, 1, 8000).unwrap(),
        ];
        let mut p = rate_promo(1, Scope::SINGLE_ITEM, 0.5);
        p.target_skus = vec![1];
        // 上限只允许 5%（原价 10000 → 500 分），促销 1000 分必须被削到 500。
        let r = calc_promotions(&mut lines, None, &[p], 0.95).unwrap();
        assert_eq!(r.total_discount_cents, 500);
        assert_eq!(lines[0].discount_cents, 500);
        assert_eq!(lines[1].discount_cents, 0, "非活动商品不得被摊到优惠");
        assert!(r.skipped.iter().any(|s| s.contains("叠加上限")));
    }

    #[test]
    fn stack_limit_caps_discount() {
        let mut lines = vec![CartLine::new(1, 1, 10000).unwrap()];
        let r = calc_promotions(
            &mut lines,
            None,
            &[promo(1, Scope::ORDER_LEVEL, 9000, false)],
            0.8,
        )
        .unwrap();
        assert!(r.total_discount_cents <= 2000, "优惠不得超过原价的 20%");
        assert!(r.skipped.iter().any(|s| s.contains("叠加上限")));
    }

    #[test]
    fn exclusive_only_highest_priority() {
        let mut lines = vec![CartLine::new(1, 1, 10000).unwrap()];
        let mut p1 = promo(1, Scope::ORDER_LEVEL, 1000, true);
        p1.priority = 1;
        let mut p2 = promo(2, Scope::ORDER_LEVEL, 2000, true);
        p2.priority = 2;
        let r = calc_promotions(&mut lines, None, &[p1, p2], 0.8).unwrap();
        assert_eq!(r.applied.len(), 1);
        assert_eq!(r.applied[0].promotion_id, 1);
    }

    #[test]
    fn single_item_flat_discount_applies_once() {
        let mut lines = vec![
            CartLine::new(1, 1, 5000).unwrap(),
            CartLine::new(2, 1, 5000).unwrap(),
        ];
        let r = calc_promotions(
            &mut lines,
            None,
            &[promo(1, Scope::SINGLE_ITEM, 1000, false)],
            0.8,
        )
        .unwrap();
        assert_eq!(r.total_discount_cents, 1000, "单品满减只能减一次");
        assert_eq!(lines.iter().map(|l| l.discount_cents).sum::<i64>(), 1000);
        assert_eq!(r.applied[0].allocations.len(), 1, "满减只落一行");
    }

    #[test]
    fn order_level_min_qty_enforced() {
        let mut lines = vec![CartLine::new(1, 1, 5000).unwrap()];
        let mut p = promo(1, Scope::ORDER_LEVEL, 1000, false);
        p.min_qty = 3;
        let r = calc_promotions(&mut lines, None, &[p], 0.8).unwrap();
        assert_eq!(r.total_discount_cents, 0);
        assert!(r.skipped.iter().any(|s| s.contains("未达门槛")));
    }

    #[test]
    fn refund_line_partial_then_clear() {
        let mut line = CartLine::new(1, 10, 1000).unwrap();
        line.discount_cents = 1000; // payable = 9000
        let first = refund_for_line(&line, 3, 0).unwrap();
        assert_eq!(first, 2700);
        let rest = refund_for_line(&line, 7, 3).unwrap();
        assert_eq!(first + rest, 9000, "累计退款恰等于行应付");
    }

    #[test]
    fn gift_line_never_discounted() {
        let mut line = CartLine::new(1, 1, 1000).unwrap();
        line.is_gift = true;
        let r = calc_promotions(
            std::slice::from_mut(&mut line),
            None,
            &[promo(1, Scope::ORDER_LEVEL, 500, false)],
            0.8,
        )
        .unwrap();
        assert_eq!(r.total_discount_cents, 0);
        assert_eq!(line.discount_cents, 0);
    }
}
