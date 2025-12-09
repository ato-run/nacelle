#[cfg(test)]
mod tests {
    use crate::workload::manifest_loader::load_manifest_str;

    #[test]
    fn parse_json_adep() {
        let json =
            r#"{"name":"test","scheduling":{},"compute":{"image":"image:tag","args":[],"env":[]}}"#;
        let (m, r) = load_manifest_str(None, json).unwrap();
        assert_eq!(m.name, "test");
        assert!(r.is_none());
    }

    #[cfg(feature = "toml-support")]
    #[test]
    fn parse_toml_capsule() {
        let toml = r#"[capsule]
name = "meeting-summarizer"
version = "0.1.0"

[resources]
cpu_cores = 2
memory = "4GB"
gpu_memory_min = "8GB"
"#;
        let (m, r) = load_manifest_str(None, toml).unwrap();
        assert_eq!(m.name, "meeting-summarizer");
        assert!(r.is_some());
        let req = r.unwrap();
        assert_eq!(req.cpu_cores, Some(2));
        assert!(req.memory_bytes.is_some());
    }
}
