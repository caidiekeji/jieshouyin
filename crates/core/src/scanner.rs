//! 扫码枪楔入与称重（§15）。
//! 楔入判定：连续字符间隔 < 阈值且以 Enter 结束 → 扫码；阈值可配（默认 50ms）。
//! 称重店内码：EAN-13，前缀 22=重量、24=金额，MOD10 校验。

#[derive(Debug, Clone)]
pub struct KeyEvent {
    pub ch: char,
    pub at_ms: u64,
    pub is_enter: bool,
}

/// 从按键序列中判定扫码。返回清洗后的条码，或 None（判定为人工输入）。
pub fn detect_scan(
    events: &[KeyEvent],
    gap_ms: u64,
    prefix_to_strip: Option<char>,
    suffix_to_strip: Option<char>,
) -> Option<String> {
    if events.is_empty() || !events.last()?.is_enter {
        return None;
    }
    let body = &events[..events.len() - 1];
    if body.is_empty() {
        return None;
    }
    for pair in body.windows(2) {
        if pair[1].at_ms.saturating_sub(pair[0].at_ms) >= gap_ms {
            return None; // 间隔超阈值 → 人工输入
        }
    }
    let mut code: String = body.iter().map(|e| e.ch).collect();
    if let Some(p) = prefix_to_strip {
        if code.starts_with(p) {
            code.remove(0);
        }
    }
    if let Some(s) = suffix_to_strip {
        if code.ends_with(s) {
            code.pop();
        }
    }
    if code.is_empty() {
        None
    } else {
        Some(code)
    }
}

/// 校验配置阈值合法性（20–200ms）。
pub fn validate_gap_ms(ms: u64) -> bool {
    (20..=200).contains(&ms)
}

/// EAN-13 MOD10 校验位。
pub fn ean13_check_digit(first_12: &[u8]) -> Option<u8> {
    if first_12.len() != 12 || first_12.iter().any(|&b| !b.is_ascii_digit()) {
        return None;
    }
    let mut sum = 0u32;
    for (i, &b) in first_12.iter().rev().enumerate() {
        let d = (b - b'0') as u32;
        sum += if i % 2 == 0 { d * 3 } else { d };
    }
    Some(((10 - (sum % 10)) % 10) as u8)
}

#[derive(Debug, Clone, PartialEq)]
pub enum WeighMode {
    Amount, // 24 前缀：金额字段为分，直接采信
    Weight, // 22 前缀：金额字段为克
}

#[derive(Debug, Clone, PartialEq)]
pub struct WeighedCode {
    pub instore_code: String,
    pub mode: WeighMode,
    pub value: i64,
}

/// 解析称重店内码（§15.2）。EAN-13：前缀2 + 商品代码5 + 数量/金额5 + 校验1。
pub fn parse_weighed_code(code: &str) -> Option<WeighedCode> {
    let bytes = code.as_bytes();
    if bytes.len() != 13 || bytes.iter().any(|&b| !b.is_ascii_digit()) {
        return None;
    }
    let check = ean13_check_digit(&bytes[..12])?;
    if check != bytes[12] - b'0' {
        return None; // 校验位不过，拒绝
    }
    let prefix = (bytes[0] - b'0') * 10 + (bytes[1] - b'0');
    let mode = match prefix {
        22 => WeighMode::Weight,
        24 => WeighMode::Amount,
        _ => return None,
    };
    let instore = std::str::from_utf8(&bytes[2..7]).ok()?.to_string();
    let value: i64 = std::str::from_utf8(&bytes[7..12]).ok()?.parse().ok()?;
    Some(WeighedCode {
        instore_code: instore,
        mode,
        value,
    })
}

/// 计算称重行金额。重量模式向上取整到分（保守向店家）；金额模式直接采信。
pub fn weighed_line_total(unit_price_cents_per_kg: i64, code: &WeighedCode) -> Option<(i64, i64)> {
    match code.mode {
        WeighMode::Amount => Some((1, code.value)), // qty=1, unit_price=金额
        WeighMode::Weight => {
            let grams = code.value;
            if grams == 0 || grams > 99999 {
                return None;
            }
            let cents = (unit_price_cents_per_kg as i128 * grams as i128 + 999) / 1000;
            i64::try_from(cents).ok().map(|c| (grams, c))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(code: &str) -> Vec<KeyEvent> {
        let mut v: Vec<KeyEvent> = code
            .chars()
            .enumerate()
            .map(|(i, ch)| KeyEvent {
                ch,
                at_ms: (i as u64) * 10,
                is_enter: false,
            })
            .collect();
        v.push(KeyEvent {
            ch: '\n',
            at_ms: code.len() as u64 * 10,
            is_enter: true,
        });
        v
    }

    #[test]
    fn detect_fast_scan() {
        let ev = scan("6901234567892");
        assert_eq!(
            detect_scan(&ev, 50, None, None).as_deref(),
            Some("6901234567892")
        );
    }

    #[test]
    fn slow_input_is_not_scan() {
        let mut ev = scan("123");
        ev[2].at_ms = 1000;
        assert!(detect_scan(&ev, 50, None, None).is_none());
    }

    #[test]
    fn no_enter_is_not_scan() {
        let mut ev = scan("123");
        ev.pop();
        assert!(detect_scan(&ev, 50, None, None).is_none());
    }

    #[test]
    fn prefix_suffix_stripped() {
        let ev = scan("%ABC*");
        assert_eq!(
            detect_scan(&ev, 50, Some('%'), Some('*')).as_deref(),
            Some("ABC")
        );
    }

    #[test]
    fn gap_range() {
        assert!(validate_gap_ms(50));
        assert!(!validate_gap_ms(10));
        assert!(!validate_gap_ms(500));
    }

    fn ean13(prefix2: &str, item: &str, val: &str) -> String {
        let first12 = format!("{prefix2}{item}{val}");
        let d = ean13_check_digit(first12.as_bytes()).unwrap();
        format!("{first12}{d}")
    }

    #[test]
    fn weighed_amount_mode() {
        let code = ean13("24", "00123", "01250");
        let parsed = parse_weighed_code(&code).unwrap();
        assert_eq!(parsed.mode, WeighMode::Amount);
        assert_eq!(parsed.instore_code, "00123");
        assert_eq!(parsed.value, 1250);
        let (qty, total) = weighed_line_total(999999, &parsed).unwrap();
        assert_eq!((qty, total), (1, 1250));
    }

    #[test]
    fn weighed_weight_mode_rounds_up() {
        let code = ean13("22", "00123", "00100");
        let parsed = parse_weighed_code(&code).unwrap();
        assert_eq!(parsed.mode, WeighMode::Weight);
        // 10.00 元/kg = 1000 分/kg，100g → 100 分
        let (grams, total) = weighed_line_total(1000, &parsed).unwrap();
        assert_eq!((grams, total), (100, 100));
        // 不能整除时向上取整：1001 分/kg × 1g → 2 分
        let (_, total2) = weighed_line_total(
            1001,
            &parse_weighed_code(&ean13("22", "00123", "00001")).unwrap(),
        )
        .unwrap();
        assert_eq!(total2, 2);
    }

    #[test]
    fn bad_checksum_rejected() {
        let mut code = ean13("24", "00123", "01250");
        code.replace_range(12..13, "0");
        // 若恰好校验位就是 0，换一位
        let last = ean13_check_digit(&code.as_bytes()[..12]).unwrap();
        if code.as_bytes()[12] == last {
            code.replace_range(12..13, &((last + 1) % 10).to_string());
        }
        assert!(parse_weighed_code(&code).is_none());
    }

    #[test]
    fn non_instore_prefix_rejected() {
        let code = ean13("69", "00123", "01250");
        assert!(parse_weighed_code(&code).is_none());
    }
}
