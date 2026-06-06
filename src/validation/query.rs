use std::collections::HashMap;

use serde_json::{Map, Value};

use super::ValidationError;

/// Build a JSON input object from URL query parameters for GET endpoints.
pub fn query_params_to_input(
    input_schema: &Value,
    query: Option<&str>,
) -> Result<Value, ValidationError> {
    let properties = input_schema
        .as_object()
        .and_then(|obj| obj.get("properties"))
        .and_then(|p| p.as_object())
        .ok_or_else(|| {
            ValidationError::SchemaCompilation("'inputSchema.properties' must be an object".into())
        })?;

    let pairs = parse_query_pairs(query);
    let mut input = Map::new();

    for (name, prop_schema) in properties {
        if let Some(raw_values) = pairs.get(name) {
            input.insert(
                name.clone(),
                coerce_query_value(prop_schema, raw_values).map_err(|msg| {
                    ValidationError::SchemaCompilation(format!("query parameter '{name}': {msg}"))
                })?,
            );
        }
    }

    Ok(Value::Object(input))
}

fn parse_query_pairs(query: Option<&str>) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    let Some(query) = query else {
        return map;
    };

    for segment in query.split('&') {
        if segment.is_empty() {
            continue;
        }

        let (raw_key, raw_value) = match segment.split_once('=') {
            Some((key, value)) => (key, value),
            None => (segment, ""),
        };

        map.entry(percent_decode(raw_key))
            .or_default()
            .push(percent_decode(raw_value));
    }

    map
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&input[i + 1..i + 3], 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }

        out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
        i += 1;
    }

    String::from_utf8_lossy(&out).into_owned()
}

fn coerce_query_value(prop_schema: &Value, raw_values: &[String]) -> Result<Value, String> {
    if let Some(enum_values) = prop_schema.get("enum").and_then(|v| v.as_array()) {
        let value = raw_values
            .first()
            .ok_or_else(|| "missing value".to_string())?;
        if enum_values.iter().any(|item| item.as_str() == Some(value)) {
            return Ok(Value::String(value.clone()));
        }
        return Err(format!("must be one of: {}", enum_display(enum_values)));
    }

    match prop_schema
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("string")
    {
        "string" => Ok(Value::String(
            raw_values
                .first()
                .cloned()
                .ok_or_else(|| "missing value".to_string())?,
        )),
        "integer" => {
            let raw = raw_values
                .first()
                .ok_or_else(|| "missing value".to_string())?;
            raw.parse::<i64>()
                .map(|n| Value::Number(n.into()))
                .map_err(|_| format!("invalid integer '{raw}'"))
        }
        "number" => {
            let raw = raw_values
                .first()
                .ok_or_else(|| "missing value".to_string())?;
            serde_json::Number::from_f64(raw.parse::<f64>().map_err(|_| format!("invalid number '{raw}'"))?)
                .map(Value::Number)
                .ok_or_else(|| format!("invalid number '{raw}'"))
        }
        "boolean" => match raw_values
            .first()
            .map(|s| s.to_ascii_lowercase())
            .as_deref()
        {
            Some("true") | Some("1") => Ok(Value::Bool(true)),
            Some("false") | Some("0") => Ok(Value::Bool(false)),
            Some(raw) => Err(format!("invalid boolean '{raw}'")),
            None => Err("missing value".to_string()),
        },
        "array" => {
            let item_schema = prop_schema
                .get("items")
                .ok_or_else(|| "array items schema is required".to_string())?;
            let values: Vec<String> = if raw_values.len() > 1 {
                raw_values.to_vec()
            } else {
                raw_values
                    .first()
                    .map(|s| s.split(',').map(str::trim).map(str::to_string).collect())
                    .unwrap_or_default()
            };

            values
                .into_iter()
                .map(|item| coerce_query_value(item_schema, &[item]))
                .collect::<Result<Vec<_>, _>>()
                .map(Value::Array)
        }
        "object" => {
            let raw = raw_values
                .first()
                .ok_or_else(|| "missing value".to_string())?;
            serde_json::from_str(raw).map_err(|err| format!("invalid JSON object: {err}"))
        }
        other => Err(format!("unsupported query parameter type '{other}'")),
    }
}

fn enum_display(values: &[Value]) -> String {
    values
        .iter()
        .filter_map(|v| v.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_scalar_query_parameters() {
        let schema = json!({
            "type": "object",
            "properties": {
                "text": { "type": "string" },
                "limit": { "type": "integer" },
                "enabled": { "type": "boolean" }
            }
        });

        let input = query_params_to_input(
            &schema,
            Some("text=hello&limit=3&enabled=true"),
        )
        .unwrap();

        assert_eq!(input["text"], "hello");
        assert_eq!(input["limit"], 3);
        assert_eq!(input["enabled"], true);
    }

    #[test]
    fn parses_repeated_array_query_parameters() {
        let schema = json!({
            "type": "object",
            "properties": {
                "tags": {
                    "type": "array",
                    "items": { "type": "string" }
                }
            }
        });

        let input =
            query_params_to_input(&schema, Some("tags=alpha&tags=beta")).unwrap();
        assert_eq!(input["tags"], json!(["alpha", "beta"]));
    }
}
