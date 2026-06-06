use airest::auth::AuthRegistry;
use airest::definitions::{
    load_endpoint_definitions_with_options, validate_endpoint_definition, LoadOptions,
};

#[test]
fn auth_registry_registers_all_shipped_strategies() {
    let registry = AuthRegistry::global();
    for strategy in [
        "none",
        "apiKey",
        "jwt",
        "oauth2Introspect",
        "trustGateway",
    ] {
        assert!(
            registry.get(strategy).is_some(),
            "missing auth strategy: {strategy}"
        );
    }
}

#[test]
fn auth_examples_load_and_validate() {
    let loaded = load_endpoint_definitions_with_options(
        std::path::Path::new("./examples/auth"),
        LoadOptions {
            recursive: false,
        },
    )
    .expect("auth examples should load");

    assert_eq!(loaded.len(), 3);
    for item in &loaded {
        validate_endpoint_definition(&item.definition)
            .unwrap_or_else(|e| panic!("{} failed validation: {e}", item.definition.name));
    }
}

#[test]
fn cache_bypass_on_guardrail_block_deserializes() {
    use airest::definitions::{minimal_test_endpoint, CacheConfig};

    let yaml = r#"
enabled: true
bypassOnGuardrailBlock: false
"#;
    let cache: CacheConfig = serde_yaml::from_str(yaml).expect("cache yaml parse");
    assert_eq!(cache.bypass_on_guardrail_block, Some(false));

    let mut endpoint = minimal_test_endpoint();
    endpoint.cache = Some(cache);
    validate_endpoint_definition(&endpoint).expect("valid cache config");
}
