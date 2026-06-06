use crate::mcp::registry::RegisteredTool;
use crate::prompts::RenderedPrompt;

const LOCAL_SERVER: &str = "local";

pub fn augment_prompt_for_tools(
    prompt: &RenderedPrompt,
    tools: &[RegisteredTool],
    native_tools: bool,
) -> RenderedPrompt {
    if native_tools || tools.is_empty() {
        return prompt.clone();
    }

    RenderedPrompt {
        system: format!("{}\n\n{}", prompt.system.trim_end(), build_tool_catalog(tools)),
        user: prompt.user.clone(),
    }
}

fn build_tool_catalog(tools: &[RegisteredTool]) -> String {
    let mut lines = vec![
        "## Available tools".to_string(),
        String::new(),
        "You may invoke these tools when needed. After tool results are provided, continue until you can return final JSON matching the output schema.".to_string(),
        String::new(),
    ];

    for tool in tools {
        lines.push(format!(
            "- **{}/{}** (`{}`): {}",
            tool.server,
            tool.tool,
            tool.api_name,
            tool.definition.description.trim()
        ));
        lines.push(format!(
            "  Parameters schema: {}",
            serde_json::to_string(&tool.definition.parameters).unwrap_or_else(|_| "{}".into())
        ));
    }

    lines.push(String::new());
    lines.push("To invoke a tool (when not using native function calling), respond with JSON only:".to_string());
    lines.push(r#"{"mcpServer":"<server>","tool":"<tool_name>","arguments":{...}}"#.to_string());
    lines.push(format!(
        r#"Use mcpServer "{LOCAL_SERVER}" for local tools (e.g. "{LOCAL_SERVER}/search_kb")."#
    ));

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{tool_api_name, ToolDefinition};

    fn sample_tool() -> RegisteredTool {
        RegisteredTool {
            qualified_name: "local/search_kb".to_string(),
            api_name: tool_api_name("local", "search_kb"),
            server: LOCAL_SERVER.to_string(),
            tool: "search_kb".to_string(),
            definition: ToolDefinition {
                name: tool_api_name("local", "search_kb"),
                description: "Search tickets".to_string(),
                parameters: serde_json::json!({"type":"object","properties":{"query":{"type":"string"}}}),
            },
            source: crate::mcp::registry::ToolSource::Local(crate::mcp::local_tool::LocalToolRuntime {
                script: "function execute() {}".to_string(),
                permissions: vec![],
                timeout_ms: 1000,
            }),
        }
    }

    #[test]
    fn augments_system_prompt_for_non_native_providers() {
        let prompt = RenderedPrompt {
            system: "You are helpful.".to_string(),
            user: "Find tickets".to_string(),
        };
        let augmented = augment_prompt_for_tools(&prompt, &[sample_tool()], false);
        assert!(augmented.system.contains("Available tools"));
        assert!(augmented.system.contains("local/search_kb"));
        assert!(augmented.system.contains("mcpServer"));
    }

    #[test]
    fn skips_augmentation_for_native_providers() {
        let prompt = RenderedPrompt {
            system: "You are helpful.".to_string(),
            user: "Find tickets".to_string(),
        };
        let augmented = augment_prompt_for_tools(&prompt, &[sample_tool()], true);
        assert_eq!(augmented.system, prompt.system);
    }
}
