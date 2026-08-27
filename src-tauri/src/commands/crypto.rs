use std::ptr;
use windows::core::PCWSTR;
use windows::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
};

// 预设的 16 字节熵值（盐） - 来自暴雪客户端的硬编码
const TOKEN_ENTROPY: [u8; 16] = [
    200, 118, 244, 174, 76, 149, 46, 254, 242, 250, 15, 84, 25, 192, 156, 67,
];

/// 将字节数组编码为十六进制字符串
pub fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// 将十六进制字符串解码为字节数组
pub fn hex_decode(hex: &str) -> Result<Vec<u8>, String> {
    if !hex.len().is_multiple_of(2) {
        return Err("hex string has odd length".to_string());
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|e| format!("invalid hex: {}", e)))
        .collect()
}

/// 使用 Windows DPAPI (Data Protection API) 加密 Token 字符串，
/// 并使用指定的盐值 (Entropy)。
/// 加密后的数据会被绑定到当前 Windows 用户 (CurrentUser)。
pub fn protect_token(token: &str) -> Result<Vec<u8>, String> {
    let mut token_bytes = token.as_bytes().to_vec();
    let data_blob = CRYPT_INTEGER_BLOB {
        cbData: token_bytes.len() as u32,
        pbData: token_bytes.as_mut_ptr(),
    };

    let mut entropy_bytes = TOKEN_ENTROPY.to_vec();
    let entropy_blob = CRYPT_INTEGER_BLOB {
        cbData: entropy_bytes.len() as u32,
        pbData: entropy_bytes.as_mut_ptr(),
    };

    let mut data_out = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };

    unsafe {
        let success = CryptProtectData(
            &data_blob,
            PCWSTR::null(),            // szDataDescr
            Some(&entropy_blob),       // pOptionalEntropy
            None,                      // pvReserved
            None,                      // pPromptStruct
            CRYPTPROTECT_UI_FORBIDDEN, // dwFlags
            &mut data_out,             // pDataOut
        );

        if success.is_err() {
            token_bytes.fill(0);
            return Err(format!("CryptProtectData failed: {:?}", success.err()));
        }

        if data_out.pbData.is_null() {
            token_bytes.fill(0);
            return Err("CryptProtectData returned empty data".to_string());
        }
        if data_out.cbData == 0 {
            token_bytes.fill(0);
            windows::Win32::Foundation::LocalFree(windows::Win32::Foundation::HLOCAL(
                data_out.pbData as *mut _,
            ));
            return Err("CryptProtectData returned empty data".to_string());
        }

        // 提取结果数据
        let slice = std::slice::from_raw_parts(data_out.pbData, data_out.cbData as usize);
        let result = slice.to_vec();
        token_bytes.fill(0);

        // 释放由 CryptProtectData 分配的内存
        windows::Win32::Foundation::LocalFree(windows::Win32::Foundation::HLOCAL(
            data_out.pbData as *mut _,
        ));

        Ok(result)
    }
}

/// 使用当前 Windows 用户的 DPAPI 凭据解密 Token。
/// 调用方负责在使用后尽快清零返回的明文；系统分配的中间缓冲区会在此处清零并释放。
pub fn unprotect_token(protected: &[u8]) -> Result<String, String> {
    if protected.is_empty() {
        return Err("DPAPI Token 为空".to_string());
    }
    let mut protected_bytes = protected.to_vec();
    let data_blob = CRYPT_INTEGER_BLOB {
        cbData: protected_bytes.len() as u32,
        pbData: protected_bytes.as_mut_ptr(),
    };
    let mut entropy_bytes = TOKEN_ENTROPY.to_vec();
    let entropy_blob = CRYPT_INTEGER_BLOB {
        cbData: entropy_bytes.len() as u32,
        pbData: entropy_bytes.as_mut_ptr(),
    };
    let mut data_out = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };

    unsafe {
        CryptUnprotectData(
            &data_blob,
            None,
            Some(&entropy_blob),
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut data_out,
        )
        .map_err(|error| format!("CryptUnprotectData failed: {error}"))?;
        if data_out.pbData.is_null() {
            return Err("CryptUnprotectData returned empty data".to_string());
        }
        if data_out.cbData == 0 {
            windows::Win32::Foundation::LocalFree(windows::Win32::Foundation::HLOCAL(
                data_out.pbData as *mut _,
            ));
            return Err("CryptUnprotectData returned empty data".to_string());
        }
        let slice = std::slice::from_raw_parts(data_out.pbData, data_out.cbData as usize);
        let plaintext_bytes = slice.to_vec();
        std::ptr::write_bytes(data_out.pbData, 0, data_out.cbData as usize);
        windows::Win32::Foundation::LocalFree(windows::Win32::Foundation::HLOCAL(
            data_out.pbData as *mut _,
        ));
        match String::from_utf8(plaintext_bytes) {
            Ok(plaintext) if !plaintext.is_empty() => Ok(plaintext),
            Ok(_) => Err("CryptUnprotectData returned an empty token".to_string()),
            Err(error) => {
                let mut bytes = error.into_bytes();
                bytes.fill(0);
                Err("CryptUnprotectData returned a non-UTF-8 token".to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{protect_token, unprotect_token};

    #[test]
    fn token_round_trips_through_current_user_dpapi() {
        let token = "test-token-for-d2rhub-export";
        let protected = protect_token(token).unwrap();
        assert_eq!(unprotect_token(&protected).unwrap(), token);
    }
}
