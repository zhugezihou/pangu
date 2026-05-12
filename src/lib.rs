//! 盘古 (Pangu) - 元Agent核心库

pub mod core;
pub mod tools;
pub mod memory;
pub mod evolution;
pub use anyhow::Result;
pub mod llm;
pub mod session;

pub mod prelude {
    pub use crate::core::{AgentResponse, Message, Role, ToolCall, ToolResult, ErrorRecord, Skill, Task};
    pub use crate::tools::{ToolRegistry, ToolExecutor};
    pub use crate::llm::provider::{LlmProvider, LlmMessage, LlmResponse, LlmTool};
}
