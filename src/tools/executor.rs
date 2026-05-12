//! 工具执行器 - 隔离执行、计时、超时控制

use std::time::Instant;
use crate::tools::registry::ToolRegistry;
use crate::core::ToolResult;

pub struct ToolExecutor {
    registry: ToolRegistry,
    default_timeout_secs: u64,
}

impl ToolExecutor {
    pub fn new(registry: ToolRegistry) -> Self {
        Self { registry, default_timeout_secs: 30 }
    }

    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.default_timeout_secs = secs;
        self
    }

    pub async fn execute(&self, name: &str, arguments: serde_json::Value, call_id: String) -> ToolResult {
        let start = Instant::now();
        let tool = match self.registry.get(name) {
            Some(t) => t,
            None => {
                return ToolResult::err(call_id,
                    format!("Tool '{}' not found in registry", name), 0);
            }
        };

        let _args_str = serde_json::to_string(&arguments).unwrap_or_default();

        match tokio::time::timeout(
            std::time::Duration::from_secs(self.default_timeout_secs),
            tool.execute(arguments),
        ).await {
            Ok(Ok(output)) => {
                let ms = start.elapsed().as_millis() as u64;
                ToolResult::ok(call_id, output, ms)
            }
            Ok(Err(e)) => {
                let ms = start.elapsed().as_millis() as u64;
                ToolResult::err(call_id, format!("Tool execution error: {}", e), ms)
            }
            Err(_) => {
                let ms = start.elapsed().as_millis() as u64;
                ToolResult::err(call_id,
                    format!("Tool '{}' timed out after {}s", name, self.default_timeout_secs), ms)
            }
        }
    }
}
