use airest::definitions::{
    load_endpoint_definitions_with_options, minimal_test_endpoint, validate_endpoint_definition,
    LocalToolSpec, LoadOptions, ToolsConfig,
};
use airest::mcp::{
    augment_prompt_for_tools, execute_local_tool, ToolRegistry, ToolSource, LOCAL_TOOL_SERVER,
};
use airest::prompts::RenderedPrompt;
use serde_json::json;

#[test]
fn loads_local_tool_example() {
    let loaded = load_endpoint_definitions_with_options(
        std::path::Path::new("./examples/mcp"),
        LoadOptions {
            recursive: false,
        },
    )
    .unwrap();
    let local = loaded
        .iter()
        .find(|l| l.definition.name == "kb-ticket-search-local")
        .expect("kb-ticket-search-local");
    let tools = local.definition.tools.as_ref().unwrap();
    assert!(tools.local.as_ref().is_some_and(|l| !l.is_empty()));
    assert!(tools
        .allow
        .as_ref()
        .is_some_and(|a| a.contains(&"local/search_kb".to_string())));
    let spec = tools.local.as_ref().unwrap().first().unwrap();
    assert!(spec.script.as_ref().is_some_and(|s| s.contains("execute")));
}

#[tokio::test]
async fn local_tool_registry_and_execution() {
    let spec = LocalToolSpec {
        name: "search_kb".to_string(),
        description: "Search KB".to_string(),
        tool_prompt: None,
        input_schema: json!({
            "type": "object",
            "properties": { "query": { "type": "string" } },
            "required": ["query"]
        }),
        runtime: "deno".to_string(),
        script: Some(
            r#"
function execute(args, host) {
  return { hits: [{ id: "T-1", query: args.query }] };
}
"#
            .to_string(),
        ),
        path: None,
        permissions: Some(vec![]),
        timeout_ms: Some(2000),
    };

    let registry = ToolRegistry::build(&ToolsConfig {
        local: Some(vec![spec]),
        allow: Some(vec!["local/search_kb".to_string()]),
        ..Default::default()
    })
    .await
    .unwrap();

    assert_eq!(registry.tools.len(), 1);
    assert_eq!(registry.tools[0].server, LOCAL_TOOL_SERVER);

    let runtime = match &registry.tools[0].source {
        ToolSource::Local(r) => r.clone(),
        _ => panic!("expected local tool"),
    };

    let out = execute_local_tool(
        &runtime,
        "search_kb",
        "req_local",
        json!({ "query": "password" }),
    )
    .await
    .unwrap();
    assert!(out["hits"].is_array());
}

#[test]
fn tool_prompt_catalog_includes_local_tools() {
    let prompt = RenderedPrompt {
        system: "Assistant.".to_string(),
        user: "Help".to_string(),
    };
    let registry = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(ToolRegistry::build(&ToolsConfig {
            local: Some(vec![LocalToolSpec {
                name: "search_kb".to_string(),
                description: "Search".to_string(),
                tool_prompt: Some("Use for KB lookup.".to_string()),
                input_schema: json!({"type":"object"}),
                runtime: "deno".to_string(),
                script: Some("function execute() { return {}; }".to_string()),
                path: None,
                permissions: None,
                timeout_ms: None,
            }]),
            allow: Some(vec!["local/search_kb".to_string()]),
            ..Default::default()
        }))
        .unwrap();

    let augmented = augment_prompt_for_tools(&prompt, &registry.tools, false);
    assert!(augmented.system.contains("local/search_kb"));
    assert!(augmented.system.contains("Use for KB lookup"));
}

#[test]
fn validates_local_tool_requires_allow_entry() {
    let mut endpoint = minimal_test_endpoint();
    endpoint.tools = Some(ToolsConfig {
        local: Some(vec![LocalToolSpec {
            name: "demo".to_string(),
            description: "Demo".to_string(),
            tool_prompt: None,
            input_schema: json!({"type":"object","properties":{}}),
            runtime: "deno".to_string(),
            script: Some("function execute() { return {}; }".to_string()),
            path: None,
            permissions: None,
            timeout_ms: None,
        }]),
        allow: None,
        ..Default::default()
    });
    assert!(validate_endpoint_definition(&endpoint).is_err());
}
