//! 金额工具：全程分为单位整数运算，禁止浮点（§14.4、§19）。
//! 行优惠分摊使用金额比例 + 最大余数法（§8.4）。

use crate::error::{CoreError, Result};

/// 分转元：带符号字符串格式化，禁止朴素写法产生 "0.-50"。
pub fn cents_to_yuan(cents: i64) -> String {
    let (sign, abs) = if cents < 0 {
        ("-", cents.saturating_neg())
    } else {
        ("", cents)
    };
    format!("{sign}{}.{:02}", abs / 100, abs % 100)
}

/// 按剩余应付金额比例分摊优惠（最大余数法）。
///
/// 硬性不变式：
/// 1. Σ分摊额 == total（一分不差）
/// 2. 每行分摊额 ≤ 该行 remaining
/// 3. remaining == 0 的行绝不 +1 分
///
/// `remaining` 为各行当前剩余应付金额（行总额 − 已分摊优惠）。
pub fn allocate_by_remaining(remaining: &[i64], total: i64) -> Result<Vec<i64>> {
    if remaining.iter().any(|&r| r < 0) {
        return Err(CoreError::Money("剩余应付金额不得为负".into()));
    }
    let base: i128 = remaining.iter().map(|&r| r as i128).sum();
    if base == 0 {
        if total == 0 {
            return Ok(vec![0; remaining.len()]);
        }
        return Err(CoreError::Money("无可分摊基数但优惠额大于零".into()));
    }
    let total_i = total as i128;
    if total_i < 0 {
        return Err(CoreError::Money("优惠额不得为负".into()));
    }

    // 超出可分摊基数说明调用方金额有误：静默截断会让“已全额分摊”的假设失真，
    // 必须报错交给上层（促销越界/退款超过实收都会在此暴露）。
    if total_i > base {
        return Err(CoreError::Money(format!(
            "分摊金额 {total} 超过可分摊基数 {base}"
        )));
    }
    let effective = total_i;
    let mut alloc: Vec<i64> = remaining
        .iter()
        .map(|&r| (effective * r as i128 / base) as i64)
        .collect();

    let allocated: i128 = alloc.iter().map(|&a| a as i128).sum();
    let rem = (effective - allocated) as usize;

    // 最大余数法：按余数降序，每行 +1 分。余数为 0 的行不参与。
    // 余数相同时按行下标升序（sort 稳定 + Reverse(余数)），结果可复现，
    // 不会因 HashMap/并行导致同一单每次分摊到不同行。
    let mut order: Vec<usize> = (0..remaining.len()).filter(|&i| remaining[i] > 0).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(effective * remaining[i] as i128 % base));
    for &i in order.iter().take(rem) {
        alloc[i] += 1;
    }

    // 断言不变式（内部自检，失败返回错误而非 panic）
    let sum: i128 = alloc.iter().map(|&a| a as i128).sum();
    if sum != effective {
        return Err(CoreError::Money(format!(
            "分摊和 {sum} != 有效优惠 {effective}"
        )));
    }
    for (&a, &r) in alloc.iter().zip(remaining.iter()) {
        if a > r || (r == 0 && a != 0) {
            return Err(CoreError::Money("分摊额穿透行金额".into()));
        }
    }
    Ok(alloc)
}

/// 混合支付退款按渠道净额占比分摊，同样使用最大余数法。
pub fn allocate_refund(weights: &[i64], total: i64) -> Result<Vec<i64>> {
    allocate_by_remaining(weights, total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cents_to_yuan_five_cases() {
        assert_eq!(cents_to_yuan(0), "0.00");
        assert_eq!(cents_to_yuan(5), "0.05");
        assert_eq!(cents_to_yuan(-5), "-0.05");
        assert_eq!(cents_to_yuan(-50), "-0.50");
        assert_eq!(cents_to_yuan(-105), "-1.05");
    }

    #[test]
    fn allocate_exact() {
        let r = allocate_by_remaining(&[100, 200, 300], 60).unwrap();
        assert_eq!(r, vec![10, 20, 30]);
    }

    #[test]
    fn allocate_largest_remainder() {
        // 优惠 100 分按 [1,1,1] 分摊 → 余数优先给前面的行
        let r = allocate_by_remaining(&[1, 1, 1], 2).unwrap();
        assert_eq!(r.iter().sum::<i64>(), 2);
        assert_eq!(r, vec![1, 1, 0]);
    }

    #[test]
    fn allocate_never_touches_zero_remaining() {
        // 已退清行 remaining=0，绝不被 +1
        let r = allocate_by_remaining(&[0, 100, 0], 7).unwrap();
        assert_eq!(r, vec![0, 7, 0]);
    }

    #[test]
    fn allocate_rejects_over_base() {
        // 超过基数不再静默截断：否则调用方以为全额已分摊
        assert!(allocate_by_remaining(&[100, 100], 500).is_err());
    }

    #[test]
    fn allocate_no_negative() {
        assert!(allocate_by_remaining(&[10, -1], 5).is_err());
    }

    #[test]
    fn allocate_stacked_no_penetration() {
        // 多促销叠加：第一行已被优惠到只剩 10
        let r1 = allocate_by_remaining(&[1000, 2000], 300).unwrap();
        let remaining2: Vec<i64> = vec![1000 - r1[0], 2000 - r1[1]];
        let r2 = allocate_by_remaining(&remaining2, 500).unwrap();
        for i in 0..2 {
            assert!(r1[i] + r2[i] <= [1000, 2000][i]);
        }
        assert_eq!(r2.iter().sum::<i64>(), 500);
    }

    #[test]
    fn refund_allocation_sums_exact() {
        let r = allocate_refund(&[3300, 1700], 1000).unwrap();
        assert_eq!(r.iter().sum::<i64>(), 1000);
    }
}
