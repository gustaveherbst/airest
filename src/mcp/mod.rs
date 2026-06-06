mod client;
mod env;
mod local_tool;
mod registry;
mod tool_loop;
mod tool_prompt;
mod transport;

pub use client::{McpClient, McpToolInfo};
pub use local_tool::execute_local_tool;
pub use registry::{ToolRegistry, ToolSource, LOCAL_TOOL_SERVER};
pub use tool_loop::run_tool_loop;
pub use tool_prompt::augment_prompt_for_tools;
