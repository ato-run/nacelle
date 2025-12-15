use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

pub const META_KEY_EGRESS_ALLOWLIST: &str = "gumball.egress_allowlist";
pub const META_KEY_EGRESS_ID_ALLOW: &str = "gumball.egress_id_allow";
pub const ENV_KEY_EGRESS_TOKEN: &str = "GUMBALL_EGRESS_TOKEN";
pub const EGRESS_CHAIN_NAME: &str = "CAPSULE_EGRESS";

#[derive(Debug, Clone)]
struct Policy {
    token: String,
    allowlist: Vec<String>,
}

#[derive(Debug, Default)]
pub struct EgressPolicyRegistry {
    policies: RwLock<HashMap<String, Policy>>,
}

impl EgressPolicyRegistry {
    pub fn global() -> &'static Self {
        static REGISTRY: OnceLock<EgressPolicyRegistry> = OnceLock::new();
        REGISTRY.get_or_init(EgressPolicyRegistry::default)
    }

    pub fn register(&self, workload_id: &str, token: String, allowlist: Vec<String>) {
        let mut policies = self.policies.write().expect("poisoned policies lock");
        policies.insert(workload_id.to_string(), Policy { token, allowlist });
    }

    pub fn unregister(&self, workload_id: &str) {
        let mut policies = self.policies.write().expect("poisoned policies lock");
        policies.remove(workload_id);
    }

    pub fn allowlist_for_basic_auth(&self, username: &str, password: &str) -> Option<Vec<String>> {
        let policies = self.policies.read().expect("poisoned policies lock");
        let policy = policies.get(username)?;
        if policy.token != password {
            return None;
        }
        Some(policy.allowlist.clone())
    }
}

pub fn parse_allowlist_csv(value: &str) -> Vec<String> {
    let mut out: Vec<String> = value
        .split(',')
        .filter_map(|s| normalize_allowlist_entry(s))
        .collect();
    out.sort();
    out.dedup();
    out
}

pub fn normalize_allowlist_entry(value: &str) -> Option<String> {
    let mut s = value.trim().to_lowercase();
    if s.is_empty() {
        return None;
    }

    if let Some(rest) = s.strip_prefix("*.") {
        s = rest.to_string();
    }
    if let Some(rest) = s.strip_prefix('.') {
        s = rest.to_string();
    }

    // If it looks like a URL, strip scheme and path.
    if let Some(idx) = s.find("://") {
        s = s[(idx + 3)..].to_string();
    }

    // Strip userinfo (e.g. user:pass@host)
    if let Some((_, host)) = s.rsplit_once('@') {
        s = host.to_string();
    }

    // Strip path
    if let Some(idx) = s.find('/') {
        s = s[..idx].to_string();
    }

    // Strip port (best-effort; handle bracketed IPv6)
    if s.starts_with('[') {
        if let Some(end) = s.find(']') {
            let host = &s[..=end];
            s = host.to_string();
        }
    } else if let Some((host, _port)) = s.rsplit_once(':') {
        // rsplit_once avoids breaking userinfo (already stripped) and keeps host part.
        s = host.to_string();
    }

    // Trim trailing dot (FQDN canonicalization)
    s = s.trim_end_matches('.').to_string();

    // Reject residual wildcards like "*" or "exa*mple.com".
    if s.contains('*') {
        return None;
    }

    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Build a shell script that enforces egress allowlist using iptables.
///
/// The script creates a dedicated chain, allows loopback/DNS/established traffic,
/// then allows the provided destinations and finally drops all remaining egress.
pub fn build_egress_enforcement_script(allowlist: &[String]) -> String {
    let mut lines = vec![
        format!("CHAIN={}", EGRESS_CHAIN_NAME),
        "set -e".to_string(),
        "iptables -N \"$CHAIN\" 2>/dev/null || true".to_string(),
        "iptables -F \"$CHAIN\"".to_string(),
        "iptables -A \"$CHAIN\" -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT".to_string(),
        "iptables -A \"$CHAIN\" -o lo -j ACCEPT".to_string(),
        "iptables -A \"$CHAIN\" -d 127.0.0.0/8 -j ACCEPT".to_string(),
        "iptables -A \"$CHAIN\" -p udp --dport 53 -j ACCEPT".to_string(),
        "iptables -A \"$CHAIN\" -p tcp --dport 53 -j ACCEPT".to_string(),
    ];

    for host in allowlist {
        if host.trim().is_empty() {
            continue;
        }
        lines.push(format!("iptables -A \"$CHAIN\" -d {} -j ACCEPT", host));
    }

    lines.push("iptables -A \"$CHAIN\" -j DROP".to_string());
    lines.push(
        "iptables -C OUTPUT -j \"$CHAIN\" 2>/dev/null || iptables -I OUTPUT 1 -j \"$CHAIN\""
            .to_string(),
    );

    lines.join("\n")
}

/// Build a shell script to clean up egress firewall rules.
pub fn build_egress_cleanup_script() -> String {
    vec![
        format!("CHAIN={}", EGRESS_CHAIN_NAME),
        "iptables -D OUTPUT -j \"$CHAIN\" 2>/dev/null || true".to_string(),
        "iptables -F \"$CHAIN\" 2>/dev/null || true".to_string(),
        "iptables -X \"$CHAIN\" 2>/dev/null || true".to_string(),
    ]
    .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_allowlist_entry_accepts_domains_and_urls() {
        assert_eq!(
            normalize_allowlist_entry("example.com"),
            Some("example.com".into())
        );
        assert_eq!(
            normalize_allowlist_entry("example.com."),
            Some("example.com".into())
        );
        assert_eq!(
            normalize_allowlist_entry("https://api.example.com/v1"),
            Some("api.example.com".into())
        );
        assert_eq!(
            normalize_allowlist_entry("https://user:pass@api.example.com:8443/v1"),
            Some("api.example.com".into())
        );
        assert_eq!(
            normalize_allowlist_entry("https://example.com.:443/path"),
            Some("example.com".into())
        );
        assert_eq!(
            normalize_allowlist_entry("http://localhost:8080/health"),
            Some("localhost".into())
        );
        assert_eq!(
            normalize_allowlist_entry("http://127.0.0.1:8080"),
            Some("127.0.0.1".into())
        );
        assert_eq!(
            normalize_allowlist_entry("*.example.com"),
            Some("example.com".into())
        );
        assert_eq!(
            normalize_allowlist_entry(".example.com"),
            Some("example.com".into())
        );
        assert_eq!(normalize_allowlist_entry("*"), None);
        assert_eq!(normalize_allowlist_entry(""), None);
    }

    #[test]
    fn parse_allowlist_csv_normalizes_and_dedupes() {
        let v = parse_allowlist_csv(
            " https://api.example.com/v1,example.com,api.example.com:443, ,*.example.com ",
        );
        assert_eq!(
            v,
            vec!["api.example.com".to_string(), "example.com".to_string()]
        );
    }

    #[test]
    fn registry_authorizes_by_username_and_token() {
        let reg = EgressPolicyRegistry::default();
        reg.register("w1", "t1".to_string(), vec!["example.com".to_string()]);

        assert_eq!(
            reg.allowlist_for_basic_auth("w1", "t1"),
            Some(vec!["example.com".to_string()])
        );
        assert_eq!(reg.allowlist_for_basic_auth("w1", "wrong"), None);
        assert_eq!(reg.allowlist_for_basic_auth("missing", "t1"), None);
    }

    #[test]
    fn egress_scripts_include_hosts_and_cleanup() {
        let script =
            build_egress_enforcement_script(&vec!["example.com".into(), "10.0.0.1".into()]);
        assert!(script.contains("iptables -A \"$CHAIN\" -d example.com -j ACCEPT"));
        assert!(script.contains("iptables -A \"$CHAIN\" -d 10.0.0.1 -j ACCEPT"));
        assert!(script.contains(EGRESS_CHAIN_NAME));

        let cleanup = build_egress_cleanup_script();
        assert!(cleanup.contains("-F \"$CHAIN\""));
        assert!(cleanup.contains(EGRESS_CHAIN_NAME));
    }
}
