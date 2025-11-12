use serde::Deserialize;

// ダミーのManifest構造体。本来は libadep-core を使う
#[derive(Deserialize)]
struct Manifest {
    name: String,
    version: String,
}

impl Manifest {
    fn from_json(json_str: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json_str)
    }

    fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() {
            return Err("Name cannot be empty".to_string());
        }
        if self.version.is_empty() {
            return Err("Version cannot be empty".to_string());
        }
        Ok(())
    }
}

/// adep.json の manifest を検証する Wasm エクスポート関数
///
/// # Arguments
/// * `json_ptr` - Wasm 線形メモリ内の JSON 文字列へのポインタ
/// * `json_len` - JSON 文字列の長さ
///
/// # Returns
/// * `1` - 検証成功
/// * `0` - 検証失敗
#[no_mangle]
pub extern "C" fn validate_manifest(json_ptr: *const u8, json_len: usize) -> u32 {
    let json_bytes = unsafe { std::slice::from_raw_parts(json_ptr, json_len) };
    let json_str = match std::str::from_utf8(json_bytes) {
        Ok(s) => s,
        Err(_) => return 0, // Invalid UTF-8
    };

    match Manifest::from_json(json_str) {
        Ok(manifest) => {
            manifest.validate().is_ok() as u32
        }
        Err(_) => 0, // JSON parsing failed
    }
}

