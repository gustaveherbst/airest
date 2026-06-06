use std::collections::HashMap;

/// Expand `${VAR}` placeholders in MCP header values from the process environment.
/// Headers with unresolved placeholders or empty values are omitted.
pub fn expand_mcp_headers(headers: Option<HashMap<String, String>>) -> HashMap<String, String> {
    let Some(headers) = headers else {
        return HashMap::new();
    };

    headers
        .into_iter()
        .filter_map(|(key, value)| {
            let expanded = expand_env_placeholders(&value)?;
            if expanded.trim().is_empty() {
                None
            } else {
                Some((key, expanded))
            }
        })
        .collect()
}

fn expand_env_placeholders(input: &str) -> Option<String> {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        rest = &rest[start + 2..];
        let Some(end) = rest.find('}') else {
            out.push_str("${");
            out.push_str(rest);
            return Some(out);
        };
        let var = &rest[..end];
        let value = std::env::var(var).ok()?;
        out.push_str(&value);
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_and_drops_empty_headers() {
        std::env::set_var("AIREST_TEST_TOKEN", "abc123");
        let headers = expand_mcp_headers(Some(HashMap::from([(
            "Authorization".to_string(),
            "Bearer ${AIREST_TEST_TOKEN}".to_string(),
        )])));
        assert_eq!(
            headers.get("Authorization").map(String::as_str),
            Some("Bearer abc123")
        );
        std::env::remove_var("AIREST_TEST_TOKEN");

        let dropped = expand_mcp_headers(Some(HashMap::from([(
            "Authorization".to_string(),
            "Bearer ${AIREST_MCP_TEST_UNSET_VAR}".to_string(),
        )])));
        assert!(dropped.is_empty());
    }
}
