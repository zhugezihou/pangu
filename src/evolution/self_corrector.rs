//! Self-Corrector - 分析错误模式，决定是否生成新技能

use crate::core::ErrorRecord;

pub struct SelfCorrector;

impl SelfCorrector {
    pub fn new() -> Self { Self }

    /// 分析错误，决定干预策略
    pub fn analyze(&self, error: &ErrorRecord) -> CorrectionStrategy {
        let msg = error.error_message.to_lowercase();
        if msg.contains("not found") || msg.contains("no such file") {
            CorrectionStrategy::CreateTool {
                tool_name: Self::infer_tool_name(&error.task),
                reason: "Missing capability",
            }
        } else if msg.contains("timeout") {
            CorrectionStrategy::AdjustTimeout {
                suggested_timeout: 60,
            }
        } else if msg.contains("permission") || msg.contains("denied") {
            CorrectionStrategy::FixPermission {
                path: Self::extract_path(&error.context),
            }
        } else if error.attempts >= 5 {
            CorrectionStrategy::CreateTool {
                tool_name: Self::infer_tool_name(&error.task),
                reason: "Repeated failure indicates missing tool",
            }
        } else {
            CorrectionStrategy::RetryWithContext {
                hint: Self::generate_hint(&error),
            }
        }
    }

    fn infer_tool_name(task: &str) -> String {
        let task_lower = task.to_lowercase();
        if task_lower.contains("search") || task_lower.contains("web") {
            "web_search".to_string()
        } else if task_lower.contains("file") || task_lower.contains("write") || task_lower.contains("read") {
            "file".to_string()
        } else if task_lower.contains("run") || task_lower.contains("execute") || task_lower.contains("shell") {
            "shell".to_string()
        } else {
            "custom_tool".to_string()
        }
    }

    fn extract_path(context: &str) -> String {
        context.lines()
            .find(|l| l.contains('/') || l.contains('\\'))
            .map(|l| l.trim().to_string())
            .unwrap_or_default()
    }

    fn generate_hint(error: &ErrorRecord) -> String {
        format!(
            "Task '{}' failed with: {}. Context: {}",
            error.task, error.error_message, &error.context[..error.context.len().min(200)]
        )
    }
}

#[derive(Debug)]
pub enum CorrectionStrategy {
    /// 需要创建新工具
    CreateTool { tool_name: String, reason: &'static str },
    /// 需要调整超时
    AdjustTimeout { suggested_timeout: u64 },
    /// 需要修复权限
    FixPermission { path: String },
    /// 带上下文提示重试
    RetryWithContext { hint: String },
}
