//! ESC/POS 打印（§7）。自研字节流构建器；默认 GB18030 编码，
//! BMP 外生僻字回退位图；打印失败进入持久化重打队列，不阻断收银。

use crate::error::{CoreError, Result};
use crate::time::format_utc;
use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

// 关键指令（§7.5）
pub const ESC_INIT: &[u8] = &[0x1B, 0x40];
pub const ESC_CHARSET_GB18030: &[u8] = &[0x1B, 0x74, 0x31]; // ESC t 49
const ESC_ALIGN: &[u8] = &[0x1B, 0x61]; // ESC a n
const ESC_BOLD: &[u8] = &[0x1B, 0x45];
const GS_SIZE: &[u8] = &[0x1D, 0x21];
const GS_CUT_FULL: &[u8] = &[0x1D, 0x56, 0x00];
const GS_CUT_PARTIAL: &[u8] = &[0x1D, 0x56, 0x01];
const ESC_KICK: &[u8] = &[0x1B, 0x70, 0x00, 0x25, 0x78];
const GS_V0: &[u8] = &[0x1D, 0x76, 0x30]; // GS v 0 光栅位图

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Block {
    #[serde(rename = "text")]
    Text {
        content: String,
        #[serde(default)]
        align: Option<String>,
        #[serde(default)]
        bold: bool,
        #[serde(default)]
        size: u8,
    },
    #[serde(rename = "divider")]
    Divider {
        #[serde(default = "default_divider")]
        char: String,
    },
    #[serde(rename = "barcode")]
    Barcode {
        content: String,
        #[serde(default = "default_symbology")]
        symbology: String,
        #[serde(default)]
        height: u8,
    },
    #[serde(rename = "qrcode")]
    Qrcode {
        content: String,
        #[serde(default = "default_qr_size")]
        size: u8,
    },
    #[serde(rename = "feed")]
    Feed { lines: u8 },
    #[serde(rename = "cut")]
    Cut {
        #[serde(default = "default_cut_mode")]
        mode: String,
    },
    #[serde(rename = "kick")]
    Kick {
        #[serde(default)]
        pin: u8,
    },
}

fn default_divider() -> String {
    "-".into()
}
fn default_symbology() -> String {
    "CODE128".into()
}
fn default_qr_size() -> u8 {
    4
}
fn default_cut_mode() -> String {
    "partial".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Template {
    pub template_id: String,
    pub paper_width_mm: i32,
    pub sections: serde_json::Value,
}

impl Template {
    /// 按 header → items → summary → footer 展开所有块。
    pub fn iter_blocks(&self) -> Result<Vec<Block>> {
        let mut out = Vec::new();
        for key in ["header", "items", "summary", "footer"] {
            if let Some(arr) = self.sections.get(key).and_then(|v| v.as_array()) {
                for v in arr {
                    let b: Block = serde_json::from_value(v.clone())?;
                    out.push(b);
                }
            }
        }
        Ok(out)
    }
}

/// 渲染变量。未知变量返回 Err(UnknownVariable)，不静默替换为空串。
pub fn render_vars(text: &str, data: &std::collections::HashMap<String, String>) -> Result<String> {
    let mut result = text.to_string();
    let mut i = 0;
    while let Some(start) = result[i..].find("{{") {
        let abs_start = i + start;
        let rest = &result[abs_start + 2..];
        let end = rest
            .find("}}")
            .ok_or_else(|| CoreError::UnknownVariable(text.into()))?;
        let name = &rest[..end];
        let value = data
            .get(name)
            .ok_or_else(|| CoreError::UnknownVariable(name.into()))?;
        result.replace_range(abs_start..abs_start + 2 + end + 2, value);
        i = abs_start + value.len();
    }
    Ok(result)
}

/// 渲染收据为字节流。
pub fn render_receipt(
    template: &Template,
    data: &std::collections::HashMap<String, String>,
) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    buf.extend_from_slice(ESC_INIT);
    buf.extend_from_slice(ESC_CHARSET_GB18030);
    let chars_per_line = if template.paper_width_mm >= 80 {
        48
    } else {
        32
    };
    for block in template.iter_blocks()? {
        match block {
            Block::Text {
                content,
                align,
                bold,
                size,
            } => {
                if let Some(a) = align {
                    let n = match a.as_str() {
                        "center" => 1,
                        "right" => 2,
                        _ => 0,
                    };
                    buf.extend_from_slice(ESC_ALIGN);
                    buf.push(n);
                }
                if bold {
                    buf.extend_from_slice(ESC_BOLD);
                    buf.push(1);
                }
                if size > 0 {
                    buf.extend_from_slice(GS_SIZE);
                    buf.push(size.min(3));
                }
                let text = render_vars(&content, data)?;
                buf.extend_from_slice(&encode_gb18030_or_bitmap(&text)?);
                buf.extend_from_slice(&[0x0A]);
                // 复位
                buf.extend_from_slice(ESC_BOLD);
                buf.push(0);
                buf.extend_from_slice(ESC_ALIGN);
                buf.push(0);
                buf.extend_from_slice(GS_SIZE);
                buf.push(0);
            }
            Block::Divider { char } => {
                let c = char.chars().next().unwrap_or('-');
                let line: String = std::iter::repeat(c).take(chars_per_line).collect();
                buf.extend_from_slice(line.as_bytes());
                buf.push(0x0A);
            }
            Block::Barcode {
                content,
                symbology,
                height,
            } => {
                if symbology != "CODE128" {
                    return Err(CoreError::Validation(format!(
                        "不支持的条码类型: {symbology}"
                    )));
                }
                let bars = render_code128(&content)?;
                buf.extend_from_slice(&[0x1D, 0x68, height.max(24)]); // GS h
                buf.extend_from_slice(&[0x1D, 0x77, 0x02]); // GS w 2
                buf.extend_from_slice(&[0x1D, 0x48, 0x02]); // GS H 2 (下方文字)
                buf.extend_from_slice(&[0x1D, 0x6B, 0x49]); // GS k 73 CODE128
                buf.push(bars.len() as u8);
                buf.extend_from_slice(&bars);
            }
            Block::Qrcode { content, size } => {
                let qr = QrCode::new(content.as_bytes())
                    .map_err(|e| CoreError::Print(format!("二维码生成失败: {e}")))?;
                let modules = qr.width();
                let colors = qr.to_colors();
                let dark = qrcode::types::Color::Dark;
                let scale = size.max(1) as usize;
                let dots = modules * scale;
                let x = (dots / 8) as u8;
                let y = (dots / 8 + usize::from(dots % 8 > 0)) as u8;
                buf.extend_from_slice(GS_V0);
                buf.extend_from_slice(&[0x00, x, 0x00, y]);
                let row_bytes = (dots + 7) / 8;
                for src_row in 0..modules {
                    for _ in 0..scale {
                        for byte in 0..row_bytes {
                            let mut b = 0u8;
                            for bit in 0..8 {
                                let col = byte * 8 + bit;
                                if col < dots {
                                    let src_col = col / scale;
                                    if colors[src_row * modules + src_col] == dark {
                                        b |= 0x80 >> bit;
                                    }
                                }
                            }
                            buf.push(b);
                        }
                    }
                }
            }
            Block::Feed { lines } => {
                for _ in 0..lines {
                    buf.push(0x0A);
                }
            }
            Block::Cut { mode } => {
                if mode == "full" {
                    buf.extend_from_slice(GS_CUT_FULL);
                } else {
                    buf.extend_from_slice(GS_CUT_PARTIAL);
                }
            }
            Block::Kick { pin } => {
                buf.extend_from_slice(&[0x1B, 0x70, 0x00, pin, 0x25, 0x78]);
                let _ = ESC_KICK;
            }
        }
    }
    Ok(buf)
}

use qrcode::QrCode;

/// GB18030（用 encoding_rs 的 GBK，覆盖常用汉字）；不可编码字符回退位图占位。
fn encode_gb18030_or_bitmap(text: &str) -> Result<Vec<u8>> {
    // 先整体尝试 GBK 编码（覆盖常用汉字）；无无法编码字符则直接返回。
    let mut whole = Vec::new();
    let (_, _, _, whole_had_errors) = encoding_rs::GBK
        .new_encoder()
        .encode_from_utf8(text, &mut whole, true);
    if !whole_had_errors {
        return Ok(whole);
    }
    // 位图回退：逐字符编码，不可编码字符生成 GS v 0 点阵占位。
    let mut out = Vec::new();
    for ch in text.chars() {
        let mut single = [0u8; 4];
        let s = ch.encode_utf8(&mut single);
        let mut enc = Vec::new();
        let (_, _, _, had_errors) = encoding_rs::GBK
            .new_encoder()
            .encode_from_utf8(s, &mut enc, true);
        if !had_errors {
            out.extend_from_slice(&enc);
        } else {
            out.extend_from_slice(GS_V0);
            out.extend_from_slice(&[0x00, 0x02, 0x00, 0x02]); // 16x16 dots
            for row in 0..2u8 {
                for col in 0..2u8 {
                    out.push(if row == 0 || col == 0 { 0x80 } else { 0x01 });
                }
            }
        }
    }
    Ok(out)
}

/// CODE128 子集 B 简化编码：起始码 104，字符值 = ASCII − 32，校验和，结束码 106。
fn render_code128(data: &str) -> Result<Vec<u8>> {
    let mut values = vec![104u8]; // START B
    for &b in data.as_bytes() {
        if b < 32 || b > 126 {
            return Err(CoreError::Print("CODE128 B 仅支持可打印 ASCII".into()));
        }
        values.push(b - 32);
    }
    let checksum: u8 = values
        .iter()
        .enumerate()
        .map(|(i, &v)| v * if i == 0 { 1 } else { i as u8 })
        .fold(0u16, |acc, v| acc + v as u16) as u8
        % 103;
    values.push(checksum);
    values.push(106); // STOP
    Ok(values)
}

/// 打印失败入队：持久化重打，不阻断收银。
pub fn enqueue_retry(
    conn: &Connection,
    order_no: &str,
    template_id: &str,
    error: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO print_retry_queue(order_no,template_id,status,retry_count,last_error,created_at)
         VALUES (?1,?2,'PENDING',0,?3,?4)",
        params![order_no, template_id, error, format_utc(&Utc::now())],
    )?;
    Ok(())
}

/// 取待重打队列（v1 仅蓄水，UI 手动触发重打）。
pub fn pending_retries(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT order_no FROM print_retry_queue WHERE status='PENDING' ORDER BY id")?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    let mut v = Vec::new();
    for row in rows {
        v.push(row?);
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpl(id: &str, json: &str) -> Template {
        Template {
            template_id: id.into(),
            paper_width_mm: 58,
            sections: serde_json::from_str(json).unwrap(),
        }
    }

    #[test]
    fn render_basic_receipt_contains_commands() {
        let t = tmpl(
            "r",
            r#"{"header":[{"type":"text","content":"店铺","align":"center","bold":true}],"footer":[{"type":"cut","mode":"partial"},{"type":"kick","pin":0}]}"#,
        );
        let bytes = render_receipt(&t, &std::collections::HashMap::new()).unwrap();
        assert!(bytes.starts_with(ESC_INIT));
        assert!(bytes.windows(3).any(|w| w == ESC_CHARSET_GB18030));
        assert!(bytes.windows(3).any(|w| w == GS_CUT_PARTIAL));
        assert!(bytes
            .windows(5)
            .any(|w| w == &[0x1B, 0x70, 0x00, 0x00, 0x25]));
    }

    #[test]
    fn unknown_variable_errors() {
        let t = tmpl(
            "r",
            r#"{"header":[{"type":"text","content":"{{unknown}}"}]}"#,
        );
        let err = render_receipt(&t, &std::collections::HashMap::new()).unwrap_err();
        assert!(matches!(err, CoreError::UnknownVariable(_)));
    }

    #[test]
    fn render_vars_substitutes() {
        let mut data = std::collections::HashMap::new();
        data.insert("store_name".to_string(), "好店".to_string());
        assert_eq!(
            render_vars("{{store_name}} 欢迎", &data).unwrap(),
            "好店 欢迎"
        );
    }

    #[test]
    fn barcode_has_code128_header() {
        let t = tmpl("r", r#"{"items":[{"type":"barcode","content":"ABC123"}]}"#);
        let bytes = render_receipt(&t, &std::collections::HashMap::new()).unwrap();
        assert!(bytes.windows(3).any(|w| w == &[0x1D, 0x6B, 0x49]));
    }

    #[test]
    fn qrcode_has_gs_v0() {
        let t = tmpl(
            "r",
            r#"{"footer":[{"type":"qrcode","content":"https://pay.example/1"}]}"#,
        );
        let bytes = render_receipt(&t, &std::collections::HashMap::new()).unwrap();
        assert!(bytes.windows(3).any(|w| w == GS_V0));
    }

    #[test]
    fn retry_queue_persists() {
        let mut c = crate::db::open_in_memory().unwrap();
        crate::db::run_migrations(&mut c).unwrap();
        enqueue_retry(&c, "O1", "receipt_default", "printer offline").unwrap();
        assert_eq!(pending_retries(&c).unwrap(), vec!["O1".to_string()]);
    }

    #[test]
    fn gb18030_chinese_encodes() {
        let b = encode_gb18030_or_bitmap("你好").unwrap();
        assert_eq!(b.len(), 4);
    }
}
