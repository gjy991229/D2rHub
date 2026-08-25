use std::ptr;
use windows::core::PCWSTR;
use windows::Win32::Security::Cryptography::{
    CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
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
            return Err(format!("CryptProtectData failed: {:?}", success.err()));
        }

        if data_out.pbData.is_null() || data_out.cbData == 0 {
            return Err("CryptProtectData returned empty data".to_string());
        }

        // 提取结果数据
        let slice = std::slice::from_raw_parts(data_out.pbData, data_out.cbData as usize);
        let result = slice.to_vec();

        // 释放由 CryptProtectData 分配的内存
        windows::Win32::Foundation::LocalFree(windows::Win32::Foundation::HLOCAL(
            data_out.pbData as *mut _,
        ));

        Ok(result)
    }
}
