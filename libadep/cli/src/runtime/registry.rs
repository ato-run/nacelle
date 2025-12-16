use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunningAdep {
    pub name: String,
    pub family_id: String,
    pub version: String,
    pub pid: u32,
    pub ports: HashMap<String, u16>,
    pub started_at: String, // ISO8601
    pub manifest_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AdepRegistry {
    pub running_adeps: Vec<RunningAdep>,
}

impl AdepRegistry {
    /// レジストリファイルのパス（プロジェクトローカル）
    pub fn registry_path() -> Result<PathBuf> {
        let cwd = std::env::current_dir().context("Failed to get current directory")?;
        Ok(cwd.join(".adep").join("local-registry.json"))
    }

    /// レジストリ読み込み + PID検証（簡略版）
    pub fn load() -> Result<Self> {
        let path = Self::registry_path()?;

        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&path).context("Failed to read registry.json")?;

        let mut registry: Self =
            serde_json::from_str(&content).context("Failed to parse registry.json")?;

        // PID検証（簡略版: kill -0 のみ）
        registry
            .running_adeps
            .retain(|adep| Self::is_process_alive(adep.pid));

        Ok(registry)
    }

    /// レジストリ保存（アトミック）
    pub fn save(&self) -> Result<()> {
        let path = Self::registry_path()?;

        // 親ディレクトリ作成
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).context("Failed to create .adep directory")?;
        }

        // 一時ファイルに書き込み
        let tmp_path = path.with_extension("tmp");
        let json = serde_json::to_string_pretty(self).context("Failed to serialize registry")?;

        std::fs::write(&tmp_path, json).context("Failed to write to tmp file")?;

        // アトミックにrename
        std::fs::rename(&tmp_path, &path).context("Failed to rename tmp file")?;

        Ok(())
    }

    /// ADEPを登録（既存のfamily_idは削除）
    pub fn register(&mut self, adep: RunningAdep) {
        self.running_adeps.retain(|a| a.family_id != adep.family_id);
        self.running_adeps.push(adep);
    }

    /// ADEPを登録解除
    pub fn unregister(&mut self, family_id: &str) {
        self.running_adeps.retain(|a| a.family_id != family_id);
    }

    /// 名前で検索
    #[allow(dead_code)]
    pub fn find_by_name(&self, name: &str) -> Option<&RunningAdep> {
        self.running_adeps.iter().find(|a| a.name == name)
    }

    /// family_idで検索
    pub fn find_by_family_id(&self, family_id: &str) -> Option<&RunningAdep> {
        self.running_adeps.iter().find(|a| a.family_id == family_id)
    }

    /// プロセス生存確認（簡略版: Phase 1A）
    fn is_process_alive(pid: u32) -> bool {
        #[cfg(unix)]
        {
            use std::process::Command;

            // kill -0: シグナルを送らずにプロセスの存在確認
            Command::new("kill")
                .arg("-0")
                .arg(pid.to_string())
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        }

        #[cfg(not(unix))]
        {
            // Windows: Phase 2 で実装
            // 現時点では常に true（conservative）
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_register_and_unregister() {
        let mut registry = AdepRegistry::default();

        let adep = RunningAdep {
            name: "test".to_string(),
            family_id: "uuid-1".to_string(),
            version: "1.0.0".to_string(),
            pid: std::process::id(),
            ports: HashMap::new(),
            started_at: chrono::Utc::now().to_rfc3339(),
            manifest_path: PathBuf::from("test"),
        };

        registry.register(adep.clone());
        assert_eq!(registry.running_adeps.len(), 1);

        // 同じfamily_idで登録すると上書き
        registry.register(adep.clone());
        assert_eq!(registry.running_adeps.len(), 1);

        // 登録解除
        registry.unregister("uuid-1");
        assert_eq!(registry.running_adeps.len(), 0);
    }

    #[test]
    fn test_registry_find() {
        let mut registry = AdepRegistry::default();

        let adep = RunningAdep {
            name: "test".to_string(),
            family_id: "uuid-1".to_string(),
            version: "1.0.0".to_string(),
            pid: std::process::id(),
            ports: HashMap::new(),
            started_at: chrono::Utc::now().to_rfc3339(),
            manifest_path: PathBuf::from("test"),
        };

        registry.register(adep);

        assert!(registry.find_by_name("test").is_some());
        assert!(registry.find_by_family_id("uuid-1").is_some());
        assert!(registry.find_by_name("nonexistent").is_none());
    }
}
