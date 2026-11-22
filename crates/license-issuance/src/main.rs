//! 激活码签发 CLI（§4.12）。私钥通过环境变量 CHECKOUT_PRIVATE_KEY 或 --key 提供，
//! 私钥文件不提交仓库。

use checkout_core::constants::LICENSE_ANNUAL;
use checkout_core::error::{CoreError, Result};
use checkout_core::license::payload::{parse_and_verify, LicensePayload, EXPECTED_PRODUCT};
use clap::{Parser, Subcommand};
use license_issuance::{
    generate_keypair, issue, rebind_payload, renew_payload, signing_key_from_hex,
};
use std::fs;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "checkout-license",
    version,
    about = "Checkout-R 激活码签发工具"
)]
struct Cli {
    /// 签发私钥 hex（32 字节）。优先于环境变量。
    #[arg(long, global = true)]
    key: Option<String>,
    /// 签发密钥编号，对应客户端公钥环里的 key_id（默认 1）。
    #[arg(long, global = true, default_value_t = checkout_core::license::payload::DEFAULT_KEY_ID)]
    key_id: u32,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// 生成新密钥对
    Init {
        #[arg(long, default_value = "checkout-r")]
        product: String,
        /// 输出目录
        #[arg(long, default_value = ".")]
        out_dir: PathBuf,
    },
    /// 签发激活码
    Issue {
        #[arg(long)]
        license_type: String,
        #[arg(long)]
        store_id: String,
        #[arg(long, default_value = "")]
        customer: String,
        #[arg(long)]
        valid_until: Option<String>,
        #[arg(long)]
        maintenance_until: Option<String>,
        #[arg(long, default_value = "7")]
        grace_days: u32,
        #[arg(long, default_value = "1")]
        max_devices: u32,
        #[arg(long, default_value = "pos.core,report.standard,member.basic")]
        entitlements: String,
        #[arg(long)]
        out: PathBuf,
    },
    /// 续期
    Renew {
        #[arg(long)]
        license: PathBuf,
        #[arg(long)]
        valid_until: String,
        #[arg(long)]
        out: PathBuf,
    },
    /// 换绑
    Rebind {
        #[arg(long)]
        license: PathBuf,
        #[arg(long)]
        old_fp: String,
        #[arg(long)]
        new_fp: String,
        #[arg(long)]
        out: PathBuf,
    },
}

fn get_key(key: &Option<String>) -> Result<String> {
    if let Some(k) = key {
        return Ok(k.clone());
    }
    std::env::var("CHECKOUT_PRIVATE_KEY")
        .map_err(|_| CoreError::Validation("未提供签发私钥".into()))
}

/// 读取并**验签**许可证：续期/换绑只允许在已签名载荷上操作，
/// 否则任何人手工改 base64 JSON 就能给自己续期或换绑到别的设备。
fn read_license(path: &PathBuf, public_key: &[u8; 32]) -> Result<(LicensePayload, String)> {
    let token =
        fs::read_to_string(path).map_err(|e| CoreError::Other(format!("读取许可证失败: {e}")))?;
    let token = token.trim().to_string();
    let (payload, _) = parse_and_verify(&token, public_key)?;
    Ok((payload, token))
}

fn main() {
    if let Err(e) = run() {
        eprintln!("错误: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let key = cli.key.clone();
    let key_id = cli.key_id;
    match cli.command {
        Command::Init { product, out_dir } => {
            let (sk, pk) = generate_keypair();
            fs::create_dir_all(&out_dir)
                .map_err(|e| CoreError::Other(format!("创建目录失败: {e}")))?;
            let sk_path = out_dir.join(format!("{product}_private.key"));
            let pk_path = out_dir.join(format!("{product}_public.key"));
            fs::write(&sk_path, &sk).map_err(|e| CoreError::Other(e.to_string()))?;
            fs::write(&pk_path, &pk).map_err(|e| CoreError::Other(e.to_string()))?;
            println!("私钥: {} (请离线保管，勿提交仓库)", sk_path.display());
            println!(
                "公钥: {} (注入到客户端构建的 CHECKOUT_PUBLIC_KEY)",
                pk_path.display()
            );
        }
        Command::Issue {
            license_type,
            store_id,
            customer,
            valid_until,
            maintenance_until,
            grace_days,
            max_devices,
            entitlements,
            out,
        } => {
            let sk = signing_key_from_hex(&get_key(&key)?)?;
            let now = chrono::Utc::now().to_rfc3339();
            let payload = LicensePayload {
                key_id,
                license_id: format!("lic_{}", uuid_like()),
                product: EXPECTED_PRODUCT.into(),
                license_type: license_type.clone(),
                issued_at: now,
                valid_until: if license_type == LICENSE_ANNUAL {
                    valid_until
                } else {
                    None
                },
                maintenance_until,
                grace_period_days: grace_days,
                max_devices,
                device_fingerprint: None,
                entitlements: entitlements
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect(),
                store_id,
                customer_name: customer,
            };
            let token = issue(&sk, &payload)?;
            fs::write(&out, &token).map_err(|e| CoreError::Other(e.to_string()))?;
            println!("已签发: {}", out.display());
        }
        Command::Renew {
            license,
            valid_until,
            out,
        } => {
            let sk = signing_key_from_hex(&get_key(&key)?)?;
            let (payload, _) = read_license(&license, sk.verifying_key().as_bytes())?;
            let maint = valid_until.clone();
            let renewed = renew_payload(&payload, valid_until, maint);
            let token = issue(&sk, &renewed)?;
            fs::write(&out, &token).map_err(|e| CoreError::Other(e.to_string()))?;
            println!("已续期: {}", out.display());
        }
        Command::Rebind {
            license,
            old_fp,
            new_fp,
            out,
        } => {
            let sk = signing_key_from_hex(&get_key(&key)?)?;
            let (payload, _) = read_license(&license, sk.verifying_key().as_bytes())?;
            if payload.device_fingerprint.as_deref() != Some(old_fp.as_str()) {
                return Err(CoreError::Validation("旧指纹摘要不匹配".into()));
            }
            let rebound = rebind_payload(&payload, new_fp);
            let token = issue(&sk, &rebound)?;
            fs::write(&out, &token).map_err(|e| CoreError::Other(e.to_string()))?;
            println!("已换绑: {}", out.display());
        }
    }
    Ok(())
}

fn uuid_like() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    format!("{:08x}{:04x}", rng.gen::<u32>(), rng.gen::<u16>())
}
