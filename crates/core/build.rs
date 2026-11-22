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

    // RFC 8032 测试向量：仅用于开发构建与单元测试，绝不能进发行包。
    const DEV_KEY: &str = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";
    let provided = env::var("CHECKOUT_PUBLIC_KEY")
        .ok()
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty());
    // PROFILE 由 cargo 提供：release 构建必须注入生产公钥，否则直接编译失败，
    // 避免"忘记配 Secret 也能打出带 DEV 公钥的发行包"。
    let is_release = env::var("PROFILE").map(|p| p == "release").unwrap_or(false);
    let (hex_key, is_dev_key) = match provided {
        Some(k) => (k, false),
        None => {
            if is_release {
                panic!(
                    "release 构建必须通过环境变量 CHECKOUT_PUBLIC_KEY 注入生产 Ed25519 公钥（64 位 hex）。\
                     CI 请在仓库 Secrets 中配置该变量；本地请先 export 再 build。"
                );
            }
            (DEV_KEY.to_string(), true)
        }
    };
    if is_release && hex_key.eq_ignore_ascii_case(DEV_KEY) {
        panic!("CHECKOUT_PUBLIC_KEY 仍是开发占位公钥，禁止用于 release 构建");
    }
    let bytes = hex::decode(&hex_key).expect("CHECKOUT_PUBLIC_KEY 必须是 64 位 hex");
    let arr: [u8; 32] = bytes
        .try_into()
        .expect("CHECKOUT_PUBLIC_KEY 必须解码为 32 字节");
    let items: Vec<String> = arr.iter().map(|b| format!("0x{b:02X}")).collect();

    let endpoint = env::var("ACTIVATION_ENDPOINT").unwrap_or_default();
    let endpoint = endpoint.trim().trim_end_matches('/').to_string();

    let body = format!(
        "pub const EMBEDDED_PUBLIC_KEY: [u8; 32] = [{}];\n\
         pub const EMBEDDED_ACTIVATION_ENDPOINT: &str = {:?};\n\
         pub const EMBEDDED_KEY_IS_DEV: bool = {is_dev_key};\n",
        items.join(", "),
        endpoint
    );
    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set")).join("public_key.rs");
    fs::write(out, body).expect("write public_key.rs failed");
}
