use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::path::PathBuf;

// RFC 8032 测试向量：仅用于开发构建与单元测试，绝不能进发行包。
const DEV_KEY: &str = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";

/// 生成内嵌常量：
/// - `EMBEDDED_KEYS`：公钥环 `[(key_id, [u8;32])]`，主公钥来自 CHECKOUT_PUBLIC_KEY（编号 1），
///   轮换期可注入旧公钥 CHECKOUT_PUBLIC_KEY_2 / _3 …（编号 2 / 3 …）保留重叠窗口；
/// - `EMBEDDED_PUBLIC_KEY`：1 号公钥，兼容旧调用方；
/// - `EMBEDDED_ACTIVATION_ENDPOINT`：在线激活服务根地址，来自 ACTIVATION_ENDPOINT。
///
/// 未提供公钥时使用开发占位公钥（RFC 8032 测试向量，仅用于开发与 CI 编译）。
/// 未提供激活地址时为空串：客户端会提示在「系统配置」里填写 Worker 根地址。
/// release 下每把公钥都必须配对应指纹 CHECKOUT_PUBLIC_KEY{_N}_FINGERPRINT，防止误配。
fn main() {
    println!("cargo:rerun-if-env-changed=CHECKOUT_PUBLIC_KEY");
    println!("cargo:rerun-if-env-changed=CHECKOUT_PUBLIC_KEY_FINGERPRINT");
    println!("cargo:rerun-if-env-changed=CHECKOUT_PUBLIC_KEY_2");
    println!("cargo:rerun-if-env-changed=CHECKOUT_PUBLIC_KEY_2_FINGERPRINT");
    println!("cargo:rerun-if-env-changed=ACTIVATION_ENDPOINT");

    let is_release = env::var("PROFILE").map(|p| p == "release").unwrap_or(false);

    /// 解析一把公钥为 32 字节；release 下校验非空、非 DEV、指纹匹配。
    fn decode_key(raw: Option<String>, label: &str, fingerprint_var: &str, is_release: bool) -> [u8; 32] {
        let key = raw.unwrap_or_default();
        let key = key.trim().to_string();
        if key.is_empty() {
            assert!(!is_release, "release 构建必须注入 {label}");
            return hex::decode(DEV_KEY).unwrap().try_into().unwrap();
        }
        assert!(!(is_release && key.eq_ignore_ascii_case(DEV_KEY)), "{label} 仍是开发占位公钥，禁止用于 release 构建");
        let arr: [u8; 32] = hex::decode(&key)
            .unwrap_or_else(|_| panic!("{label} 必须是 64 位 hex"))
            .try_into()
            .unwrap_or_else(|_| panic!("{label} 必须解码为 32 字节"));
        if is_release {
            let expected = env::var(fingerprint_var)
                .ok()
                .map(|s| s.trim().to_ascii_lowercase())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| panic!("release 构建必须注入 {fingerprint_var}"));
            let actual: String = Sha256::digest(arr)
                .iter()
                .take(8)
                .map(|b| format!("{b:02x}"))
                .collect();
            assert_eq!(actual, expected, "{label} 指纹不匹配：拒绝用未登记公钥发行");
        }
        arr
    }

    let primary = decode_key(
        env::var("CHECKOUT_PUBLIC_KEY").ok(),
        "CHECKOUT_PUBLIC_KEY",
        "CHECKOUT_PUBLIC_KEY_FINGERPRINT",
        is_release,
    );
    // 轮换预留：旧公钥按编号递增，release 下同样要求指纹登记。
    let mut keys = vec![(1u32, primary)];
    for id in 2..=8u32 {
        let var = format!("CHECKOUT_PUBLIC_KEY_{id}");
        let raw = env::var(&var).ok().filter(|s| !s.trim().is_empty());
        if raw.is_none() {
            break; // 编号连续，不跳号
        }
        let key = decode_key(
            raw,
            &var,
            &format!("{var}_FINGERPRINT"),
            is_release,
        );
        keys.push((id, key));
    }

    let is_dev_key = primary.as_slice() == hex::decode(DEV_KEY).unwrap().as_slice();
    let primary_items: Vec<String> = primary.iter().map(|b| format!("0x{b:02X}")).collect();
    let ring_items: Vec<String> = keys
        .iter()
        .map(|(id, k)| {
            let bytes: Vec<String> = k.iter().map(|b| format!("0x{b:02X}")).collect();
            format!("({id}u32, [{}])", bytes.join(", "))
        })
        .collect();

    let endpoint = env::var("ACTIVATION_ENDPOINT").unwrap_or_default();
    let endpoint = endpoint.trim().trim_end_matches('/').to_string();

    let body = format!(
        "pub const EMBEDDED_PUBLIC_KEY: [u8; 32] = [{}];\n\
         pub const EMBEDDED_KEYS: &[(u32, [u8; 32])] = &[{}];\n\
         pub const EMBEDDED_ACTIVATION_ENDPOINT: &str = {:?};\n\
         pub const EMBEDDED_KEY_IS_DEV: bool = {is_dev_key};\n",
        primary_items.join(", "),
        ring_items.join(", "),
        endpoint
    );
    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set")).join("public_key.rs");
    fs::write(out, body).expect("write public_key.rs failed");
}
