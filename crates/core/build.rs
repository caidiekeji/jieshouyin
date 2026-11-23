use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::path::PathBuf;

// RFC 8032 测试向量：任何人都有私钥，仅可用于单元测试，绝不能进任何构建。
const DEV_KEY: &str = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";

// 正式签发公钥：与激活服务 Worker 的 LICENSE_PRIVATE_KEY 私钥配对。
// 公钥是公开值，随每个安装包分发是行业标准做法（JWT/JWKS、证书固定同理），
// 无保密要求；内嵌进源码让构建零外部配置，发行流水线不再因漏配变量而失败。
// 指纹 = SHA256(公钥 32 字节) 前 8 字节 hex，构建时自检，防止公钥被改错。
// 密钥轮换流程见 jihuo/README.md：更新 1 号常量与指纹，或按编号填入 2/3… 号旧公钥。
const PROD_KEY_1: &str = "5816ec0d92381c7a05949604de60e32b13e9555933281957ebbb84345023d0e7";
const PROD_KEY_1_FP: &str = "31fe55eb95dbb5fe";
// 轮换期旧公钥与指纹；当前无旧钥，留空。轮换时填入并保留整个重叠窗口。
const PROD_KEY_2: &str = "";
const PROD_KEY_2_FP: &str = "";

/// 生成内嵌常量：
/// - `EMBEDDED_KEYS`：公钥环 `[(key_id, [u8;32])]`，1 号为主公钥（源码内嵌，
///   可由 CHECKOUT_PUBLIC_KEY 覆盖），轮换期按编号追加 CHECKOUT_PUBLIC_KEY_2 / _3 …；
/// - `EMBEDDED_PUBLIC_KEY`：1 号公钥，兼容旧调用方；
/// - `EMBEDDED_ACTIVATION_ENDPOINT`：在线激活服务根地址，来自 ACTIVATION_ENDPOINT。
///
/// 公钥默认取源码内嵌的正式值，因此 release 构建不依赖任何 CI 变量即可通过。
/// 环境变量仅用于密钥轮换时临时覆盖，此时 release 仍要求指纹匹配，防止误配。
/// 未提供激活地址时为空串：客户端会提示在「系统配置」里填写 Worker 根地址。
fn main() {
    println!("cargo:rerun-if-env-changed=CHECKOUT_PUBLIC_KEY");
    println!("cargo:rerun-if-env-changed=CHECKOUT_PUBLIC_KEY_FINGERPRINT");
    println!("cargo:rerun-if-env-changed=CHECKOUT_PUBLIC_KEY_2");
    println!("cargo:rerun-if-env-changed=CHECKOUT_PUBLIC_KEY_2_FINGERPRINT");
    println!("cargo:rerun-if-env-changed=ACTIVATION_ENDPOINT");

    let is_release = env::var("PROFILE").map(|p| p == "release").unwrap_or(false);

    /// 解析一把公钥为 32 字节：环境变量优先（轮换覆盖），否则用源码内嵌正式公钥。
    /// 任何来源都拒绝测试向量；release 下额外校验指纹（环境变量指纹优先，否则用内嵌指纹）。
    fn decode_key(
        raw: Option<String>,
        embedded: &str,
        embedded_fp: &str,
        label: &str,
        is_release: bool,
    ) -> [u8; 32] {
        let key = raw
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| embedded.trim().to_string());
        assert!(
            !key.eq_ignore_ascii_case(DEV_KEY),
            "{label} 是开发占位公钥，禁止用于任何构建"
        );
        let arr: [u8; 32] = hex::decode(&key)
            .unwrap_or_else(|_| panic!("{label} 必须是 64 位 hex"))
            .try_into()
            .unwrap_or_else(|_| panic!("{label} 必须解码为 32 字节"));
        if is_release {
            let expected = env::var(format!("{label}_FINGERPRINT"))
                .ok()
                .map(|s| s.trim().to_ascii_lowercase())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| embedded_fp.trim().to_ascii_lowercase());
            assert!(!expected.is_empty(), "release 构建缺少 {label} 的指纹");
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
        PROD_KEY_1,
        PROD_KEY_1_FP,
        "CHECKOUT_PUBLIC_KEY",
        is_release,
    );
    // 轮换预留：旧公钥按编号递增，不跳号；源码内嵌或环境变量提供均可。
    let mut keys = vec![(1u32, primary)];
    for id in 2..=8u32 {
        let var = format!("CHECKOUT_PUBLIC_KEY_{id}");
        let (embedded, embedded_fp) = match id {
            2 => (PROD_KEY_2, PROD_KEY_2_FP),
            _ => ("", ""),
        };
        let raw = env::var(&var).ok().filter(|s| !s.trim().is_empty());
        if raw.is_none() && embedded.is_empty() {
            break;
        }
        let key = decode_key(raw, embedded, embedded_fp, &var, is_release);
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
