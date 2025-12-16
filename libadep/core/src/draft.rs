use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct DraftInput {
    pub name: Option<String>,
    pub version: Option<String>,
    pub display_name: Option<String>,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub advanced: Option<DraftAdvanced>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct DraftAdvanced {
    #[serde(rename = "type")]
    pub type_: Option<DraftType>,
    pub start: Option<String>,
    pub port: Option<u16>,
    pub env: Option<HashMap<String, String>>,
    pub health_check: Option<String>,
    pub base_image: Option<String>, // Option to override base image if needed in future
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DraftType {
    Static,
    App, // Generic App (Web/Node/Python)
    Inference,
    Tool,
}

impl DraftInput {
    pub fn merge(&mut self, other: DraftInput) {
        if let Some(name) = other.name {
            self.name = Some(name);
        }
        if let Some(version) = other.version {
            self.version = Some(version);
        }
        if let Some(display_name) = other.display_name {
            self.display_name = Some(display_name);
        }
        if let Some(icon) = other.icon {
            self.icon = Some(icon);
        }
        if let Some(description) = other.description {
            self.description = Some(description);
        }
        if let Some(tags) = other.tags {
            self.tags = Some(tags);
        }

        if let Some(other_adv) = other.advanced {
            let self_adv = self.advanced.get_or_insert(DraftAdvanced::default());
            self_adv.merge(other_adv);
        }
    }
}

impl DraftAdvanced {
    pub fn merge(&mut self, other: DraftAdvanced) {
        if let Some(t) = other.type_ {
            self.type_ = Some(t);
        }
        if let Some(s) = other.start {
            self.start = Some(s);
        }
        if let Some(p) = other.port {
            self.port = Some(p);
        }
        if let Some(hc) = other.health_check {
            self.health_check = Some(hc);
        }
        if let Some(bi) = other.base_image {
            self.base_image = Some(bi);
        }

        if let Some(other_env) = other.env {
            let self_env = self.env.get_or_insert(HashMap::new());
            for (k, v) in other_env {
                self_env.insert(k, v);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_draft_merge() {
        let mut base = DraftInput {
            name: Some("base".to_string()),
            display_name: Some("Base".to_string()),
            advanced: Some(DraftAdvanced {
                port: Some(8080),
                ..Default::default()
            }),
            ..Default::default()
        };

        let overlay = DraftInput {
            name: Some("overlay".to_string()),
            advanced: Some(DraftAdvanced {
                port: Some(9000),
                start: Some("npm start".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        base.merge(overlay);

        assert_eq!(base.name, Some("overlay".to_string()));
        assert_eq!(base.display_name, Some("Base".to_string())); // Not overwritten
        assert_eq!(base.advanced.clone().unwrap().port, Some(9000));
        assert_eq!(
            base.advanced.as_ref().unwrap().start,
            Some("npm start".to_string())
        );
    }
}
