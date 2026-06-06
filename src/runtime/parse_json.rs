pub fn parse_model_json(content: &str, strip_fences: bool) -> Result<serde_json::Value, String> {
    let trimmed = content.trim();
    let json_str = if strip_fences {
        strip_markdown_fences(trimmed)
    } else {
        trimmed.to_string()
    };

    serde_json::from_str(&json_str).map_err(|e| e.to_string())
}

fn strip_markdown_fences(content: &str) -> String {
    let mut text = content.trim();

    if text.starts_with("```") {
        if let Some(start) = text.find('\n') {
            text = &text[start + 1..];
        }
        if let Some(end) = text.rfind("```") {
            text = &text[..end];
        }
    }

    text.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_json() {
        let value = parse_model_json(r#"{"sentiment":"positive"}"#, false).unwrap();
        assert_eq!(value["sentiment"], "positive");
    }

    #[test]
    fn strips_markdown_fences_when_enabled() {
        let raw = "```json\n{\"sentiment\":\"neutral\"}\n```";
        let value = parse_model_json(raw, true).unwrap();
        assert_eq!(value["sentiment"], "neutral");
    }

    #[test]
    fn rejects_invalid_json() {
        assert!(parse_model_json("not json", false).is_err());
    }
}
