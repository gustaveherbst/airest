use std::path::Path;

use airest::definitions::{
    load_endpoint_definitions_with_options, validate_endpoint_definition, LoadOptions,
};
use airest::guardrails::{GuardrailChain, GuardrailEngine};

#[test]
fn healthcare_and_finance_example_packs_load_with_guardrail_chains() {
    let engine = GuardrailEngine::new();

    for folder in ["./examples/healthcare", "./examples/finance"] {
        let loaded = load_endpoint_definitions_with_options(
            Path::new(folder),
            LoadOptions { recursive: true },
        )
        .unwrap_or_else(|e| panic!("load {folder}: {e}"));

        assert_eq!(loaded.len(), 1, "expected one endpoint in {folder}");
        let def = &loaded[0].definition;
        assert!(validate_endpoint_definition(def).is_ok());
        assert!(
            def.guardrails.as_ref().is_some_and(|g| !g.is_empty()),
            "expected guardrails in {folder}"
        );

        let chain = GuardrailChain::compile(
            def.guardrails.as_ref().unwrap(),
            loaded[0].source_path.parent(),
            &engine,
        )
        .unwrap_or_else(|e| panic!("compile chain for {}: {e}", def.name));
        assert!(!chain.is_empty());
    }
}

#[test]
fn bundled_examples_include_healthcare_and_finance() {
    let loaded = load_endpoint_definitions_with_options(
        Path::new("./examples"),
        LoadOptions { recursive: true },
    )
    .unwrap();
    let names: Vec<&str> = loaded.iter().map(|l| l.definition.name.as_str()).collect();
    assert!(names.contains(&"clinical-note-summary"));
    assert!(names.contains(&"payment-fraud-check"));
}
