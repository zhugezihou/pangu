//! Shell 工具 - 执行系统命令

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use crate::tools::registry::Tool;

#[derive(Serialize, Deserialize)]
pub struct ShellArgs {
    pub command: String,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    #[serde(default)]
    pub cwd: Option<String>,
}

fn default_timeout() -> u64 { 30 }

pub struct ShellTool;

impl ShellTool {
    pub fn new() -> Self { Self }
}

impl Default for ShellTool {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl Tool for ShellTool {
    fn name(&self) -> &str { "shell" }
    fn description(&self) -> &str {
        "Execute a shell command. Returns stdout on success, stderr on error."
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The shell command to execute"
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Timeout in seconds (default: 30)",
                    "default": 30
                },
                "cwd": {
                    "type": "string",
                    "description": "Working directory (optional)"
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<String> {
        let args: ShellArgs = serde_json::from_value(args)?;
        let mut executor = duct::cmd("sh", ["-c", &args.command]);
        if let Some(cwd) = &args.cwd {
            executor = executor.dir(cwd);
        }
        let output = executor
            .stdout_capture()
            .stderr_capture()
            .run()
            .map_err(|e| anyhow::anyhow!("Shell error: {}", e))?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if output.status.success() {
            Ok(stdout.to_string())
        } else {
            Err(anyhow::anyhow!("Exit {}: {}\n{}", output.status.code().unwrap_or(1), stderr, stdout))
        }
    }
}
