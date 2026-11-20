use std::env;
use std::fs;
use std::path::PathBuf;

/// 生成内嵌常量：
/// - `EMBEDDED_PUBLIC_KEY`：Ed25519 公钥（64 位 hex），来自环境变量 CHECKOUT_PUBLIC_KEY；
/// - `EMBEDDED_ACTIVATION_ENDPOINT`：在线激活服务根地址，来自环境变量 ACTIVATION_ENDPOINT。
///
/// 未提供公钥时使用开发占位公钥（RFC 8032 测试向量，仅用于开发与 CI 编译）。
/// 未提供激活地址时为空串：客户端会提示在「系统配置」里填写 Worker 根地址。
fn main() {
    println!("cargo:rerun-if-env-changed=CHECKOUT_PUBLIC_KEY");
    println!("cargo:rerun-if-env-changed=ACTIVATION_ENDPOINT");

    let hex_key = env::var("CHECKOUT_PUBLIC_KEY").unwrap_or_else(|_| {
        "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a".to_string()
    });
    let bytes = hex::decode(&hex_key).expect("CHECKOUT_PUBLIC_KEY 必须是 64 位 hex");
    let arr: [u8; 32] = bytes
        .try_into()
        .expect("CHECKOUT_PUBLIC_KEY 必须解码为 32 字节");
    let items: Vec<String> = arr.iter().map(|b| format!("0x{b:02X}")).collect();

    let endpoint = env::var("ACTIVATION_ENDPOINT").unwrap_or_default();
    let endpoint = endpoint.trim().trim_end_matches('/').to_string();

    let body = format!(
        "pub const EMBEDDED_PUBLIC_KEY: [u8; 32] = [{}];\n\
         pub const EMBEDDED_ACTIVATION_ENDPOINT: &str = {:?};\n",
        items.join(", "),
        endpoint
    );
    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set")).join("public_key.rs");
    fs::write(out, body).expect("write public_key.rs failed");
}
