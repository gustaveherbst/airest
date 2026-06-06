#[derive(Debug, Clone, Default)]
pub struct NetworkAllowlist {
    entries: Vec<String>,
    allow_all: bool,
}

impl NetworkAllowlist {
    pub fn parse(permissions: &[String]) -> Self {
        let mut entries = Vec::new();
        let mut allow_all = false;
        for perm in permissions {
            let Some(rest) = perm.strip_prefix("network:") else {
                continue;
            };
            if rest == "*" {
                allow_all = true;
            } else {
                entries.push(rest.trim().to_ascii_lowercase());
            }
        }
        Self { entries, allow_all }
    }

    pub fn is_empty(&self) -> bool {
        !self.allow_all && self.entries.is_empty()
    }

    pub fn is_allowed(&self, url: &str) -> bool {
        if self.allow_all {
            return is_http_url(url);
        }
        let Some((scheme, host)) = parse_http_host(url) else {
            return false;
        };
        self.entries
            .iter()
            .any(|entry| host_matches(entry, &host, &scheme, url))
    }
}

fn is_http_url(url: &str) -> bool {
    parse_http_host(url).is_some()
}

fn parse_http_host(url: &str) -> Option<(String, String)> {
    let (scheme, rest) = url.split_once("://")?;
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let host = rest.split('/').next()?.split(':').next()?.to_ascii_lowercase();
    Some((scheme.to_string(), host))
}

fn host_matches(entry: &str, host: &str, scheme: &str, full_url: &str) -> bool {
    if entry.contains("://") {
        let url_prefix = format!("{scheme}://{host}");
        let lower = full_url.to_ascii_lowercase();
        return lower.starts_with(&url_prefix) || lower.starts_with(entry);
    }
    host == entry || host.ends_with(&format!(".{entry}"))
}

/// Bare `fetch(` calls (not `host.fetch(`) — forbidden in hook scripts.
pub fn script_uses_global_fetch(script: &str) -> bool {
    for (idx, _) in script.match_indices("fetch(") {
        let prefix = &script[..idx];
        if prefix.ends_with("host.") {
            continue;
        }
        if idx == 0 || script.as_bytes()[idx - 1] != b'.' {
            return true;
        }
    }
    false
}

/// Any outbound HTTP via `host.fetch(` or bare `fetch(` — requires network permissions at validate time.
pub fn script_uses_network_fetch(script: &str) -> bool {
    script.contains("host.fetch(") || script_uses_global_fetch(script)
}

pub fn validate_permission_tokens(permissions: &[String]) -> Result<(), String> {
    for perm in permissions {
        if perm == "network:*" {
            return Err(
                "network:* is not allowed for hooks; use explicit hosts such as network:api.example.com"
                    .to_string(),
            );
        }
        if let Some(rest) = perm.strip_prefix("network:") {
            if rest.is_empty() {
                return Err("network permission cannot be empty".to_string());
            }
            if !rest.contains('.') && !rest.contains("://") && rest != "localhost" {
                return Err(format!(
                    "network permission '{perm}' must include a host/domain (not '*')"
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_configured_host() {
        let list = NetworkAllowlist::parse(&["network:api.example.com".to_string()]);
        assert!(list.is_allowed("https://api.example.com/v1"));
        assert!(!list.is_allowed("https://evil.example.com/v1"));
    }

    #[test]
    fn script_uses_network_fetch_detects_host_fetch() {
        assert!(script_uses_network_fetch(
            "function transform(input, host) { host.fetch('https://api.example.com'); return input; }"
        ));
        assert!(script_uses_global_fetch("fetch('https://evil')"));
        assert!(!script_uses_global_fetch(
            "function transform(input, host) { host.fetch('https://api.example.com'); return input; }"
        ));
    }

    #[test]
    fn validate_permission_tokens_rejects_network_wildcard() {
        let err = validate_permission_tokens(&["network:*".to_string()]).unwrap_err();
        assert!(err.contains("network:*"));
    }
}
