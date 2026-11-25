//! 设备指纹（§4.4）：SHA-256(MachineGuid || volume_serial)。
//! 降级纪律：计算失败不得阻止激活，降级为"未绑定设备"模式并审计。

use crate::error::Result;
use sha2::{Digest, Sha256};

/// 计算设备指纹。Windows 读取注册表 MachineGuid 与系统盘卷序列号；
/// 非 Windows 或读取失败返回 Ok(None)，由调用方按未绑定模式处理。
pub fn device_fingerprint() -> Result<Option<String>> {
    #[cfg(windows)]
    {
        let machine_guid = read_machine_guid();
        let volume_serial = read_volume_serial();
        match (machine_guid, volume_serial) {
            (Some(guid), Some(serial)) => {
                let mut hasher = Sha256::new();
                hasher.update(guid.as_bytes());
                hasher.update(serial.as_bytes());
                Ok(Some(hex::encode(hasher.finalize())))
            }
            _ => Ok(None),
        }
    }
    #[cfg(not(windows))]
    {
        Ok(None)
    }
}

#[cfg(windows)]
fn read_machine_guid() -> Option<String> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::ptr;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY_LOCAL_MACHINE, KEY_READ,
    };

    let path: Vec<u16> = "SOFTWARE\\Microsoft\\Cryptography\0"
        .encode_utf16()
        .collect();
    let name: Vec<u16> = "MachineGuid\0".encode_utf16().collect();
    let mut hkey: *mut std::ffi::c_void = ptr::null_mut();
    let rc = unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, path.as_ptr(), 0, KEY_READ, &mut hkey) };
    if rc != 0 {
        return None;
    }
    let mut len: u32 = 0;
    unsafe {
        RegQueryValueExW(
            hkey,
            name.as_ptr(),
            ptr::null(),
            ptr::null_mut(),
            ptr::null_mut(),
            &mut len,
        );
    }
    if len == 0 || len > 512 {
        unsafe { RegCloseKey(hkey) };
        return None;
    }
    let mut buf = vec![0u8; len as usize];
    let rc = unsafe {
        RegQueryValueExW(
            hkey,
            name.as_ptr(),
            ptr::null(),
            ptr::null_mut(),
            buf.as_mut_ptr(),
            &mut len,
        )
    };
    unsafe { RegCloseKey(hkey) };
    if rc != 0 {
        return None;
    }
    let units: Vec<u16> = buf
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    Some(OsString::from_wide(&units).to_string_lossy().into_owned())
}

#[cfg(windows)]
fn read_volume_serial() -> Option<String> {
    use std::ptr;
    use windows_sys::Win32::Storage::FileSystem::GetVolumeInformationW;

    let root: Vec<u16> = "C:\\\0".encode_utf16().collect();
    let mut serial: u32 = 0;
    let rc = unsafe {
        GetVolumeInformationW(
            root.as_ptr(),
            ptr::null_mut(),
            0,
            &mut serial,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            0,
        )
    };
    if rc == 0 {
        // 系统盘可能不是 C:，尝试从环境变量取
        let sys = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
        let drive = sys.chars().take(3).collect::<String>();
        let root2: Vec<u16> = format!("{drive}\0").encode_utf16().collect();
        let rc2 = unsafe {
            GetVolumeInformationW(
                root2.as_ptr(),
                ptr::null_mut(),
                0,
                &mut serial,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                0,
            )
        };
        if rc2 == 0 {
            // 指纹计算失败不影响激活，降级为未绑定
            return None;
        }
    }
    Some(format!("{:08X}", serial))
}

/// 指纹摘要（用于审计，避免存完整指纹）。
pub fn fp_digest(fp: Option<&str>) -> String {
    match fp {
        Some(f) => {
            let mut h = Sha256::new();
            h.update(f.as_bytes());
            hex::encode(h.finalize())[..16].to_string()
        }
        None => "UNBOUND".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_is_stable_and_short() {
        let d = fp_digest(Some("abc"));
        assert_eq!(d.len(), 16);
        assert_eq!(d, fp_digest(Some("abc")));
        assert_ne!(d, fp_digest(Some("abd")));
    }

    #[test]
    fn unbound_has_digest() {
        assert_eq!(fp_digest(None), "UNBOUND");
    }

    #[test]
    fn fingerprint_call_never_panics() {
        let _ = device_fingerprint();
    }
}
