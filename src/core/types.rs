//! 盘古核心类型定义

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// 会话ID
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionId(pub String);

impl SessionId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

/// 工具调用请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

impl ToolCall {
    pub fn new(name: String, arguments: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            arguments,
            created_at: Utc::now(),
        }
    }
}

/// 工具执行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub call_id: String,
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
    pub duration_ms: u64,
}

impl ToolResult {
    pub fn ok(call_id: String, output: String, duration_ms: u64) -> Self {
        Self { call_id, success: true, output, error: None, duration_ms }
    }
    pub fn err(call_id: String, error: String, duration_ms: u64) -> Self {
        Self { call_id, success: false, output: String::new(), error: Some(error), duration_ms }
    }
}

/// LLM消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_results: Option<Vec<ToolResult>>,
}

impl Message {
    pub fn system(content: &str) -> Self {
        Self { role: Role::System, content: content.to_string(), tool_calls: None, tool_results: None }
    }
    pub fn user(content: &str) -> Self {
        Self { role: Role::User, content: content.to_string(), tool_calls: None, tool_results: None }
    }
    pub fn assistant(content: &str) -> Self {
        Self { role: Role::Assistant, content: content.to_string(), tool_calls: None, tool_results: None }
    }
    pub fn tool(_call_id: &str, content: &str) -> Self {
        Self { role: Role::Tool, content: content.to_string(), tool_calls: None, tool_results: None }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// Agent 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: Option<String>,
}

/// 错误记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorRecord {
    pub id: String,
    pub session_id: String,
    pub task: String,
    pub error_type: String,
    pub error_message: String,
    pub context: String,
    pub attempts: u32,
    pub resolved: bool,
    pub created_at: DateTime<Utc>,
}

impl ErrorRecord {
    pub fn new(session_id: &str, task: &str, error_type: &str, error_message: &str, context: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            task: task.to_string(),
            error_type: error_type.to_string(),
            error_message: error_message.to_string(),
            context: context.to_string(),
            attempts: 1,
            resolved: false,
            created_at: Utc::now(),
        }
    }
    pub fn increment_attempt(&mut self) {
        self.attempts += 1;
    }
}

/// 技能/工具代码
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source_code: String,
    pub tool_name: String,
    pub parameters: serde_json::Value,
    pub success_count: u32,
    pub failure_count: u32,
    pub created_at: DateTime<Utc>,
    pub last_used: Option<DateTime<Utc>>,
}

impl Skill {
    pub fn new(name: &str, description: &str, source_code: &str, tool_name: &str, parameters: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            description: description.to_string(),
            source_code: source_code.to_string(),
            tool_name: tool_name.to_string(),
            parameters,
            success_count: 0,
            failure_count: 0,
            created_at: Utc::now(),
            last_used: None,
        }
    }
}

/// 任务描述
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub description: String,
    pub status: TaskStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    Pending,
    Running,
    Done,
    Failed,
}
