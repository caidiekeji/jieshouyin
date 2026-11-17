use std::env;
use std::fs;
use std::path::PathBuf;

/// 从 CHECKOUT_PUBLIC_KEY（64 位 hex Ed25519 公钥）生成内嵌常量。
/// 未提供时使用开发占位公钥（RFC 8032 测试向量，仅用于开发与 CI 编译）。
fn main() {
    println!("cargo:rerun-if-env-changed=CHECKOUT_PUBLIC_KEY");
    let hex_key = env::var("CHECKOUT_PUBLIC_KEY").unwrap_or_else(|_| {
        "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a".to_string()
    });
    let bytes = hex::decode(&hex_key).expect("CHECKOUT_PUBLIC_KEY 必须是 64 位 hex");
    let arr: [u8; 32] = bytes
        .try_into()
        .expect("CHECKOUT_PUBLIC_KEY 必须解码为 32 字节");
    let items: Vec<String> = arr.iter().map(|b| format!("0x{b:02X}")).collect();
    let body = format!(
        "pub const EMBEDDED_PUBLIC_KEY: [u8; 32] = [{}];\n",
        items.join(", ")
    );
    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set")).join("public_key.rs");
    fs::write(out, body).expect("write public_key.rs failed");
}
