//! Gateway 数据模型

use serde::{Deserialize, Serialize};

/// LLM 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub provider: String,       // "openai" | "anthropic" | "custom"
    pub model: String,          // "gpt-4o-mini" | "claude-3-5-sonnet" | ...
    pub api_key: String,
    #[serde(default)]
    pub base_url: String,       // OpenAI compatible, e.g. "https://api.openai.com/v1"
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_context_window")]
    pub context_window: usize,
}

fn default_temperature() -> f32 { 0.7 }
fn default_max_tokens() -> u32 { 4096 }
fn default_context_window() -> usize { 128000 }

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: "openai".to_string(),
            model: "gpt-4o-mini".to_string(),
            api_key: String::new(),
            base_url: "https://api.openai.com/v1".to_string(),
            temperature: 0.7,
            max_tokens: 4096,
            context_window: 128000,
        }
    }
}

/// Agent 执行请求
#[derive(Debug, Clone, Deserialize)]
pub struct RunRequest {
    pub task: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub max_iterations: Option<usize>,
    #[serde(default)]
    pub temperature: Option<f32>,
}

/// Agent 执行响应
#[derive(Debug, Clone, Serialize)]
pub struct RunResponse {
    pub session_id: String,
    pub result: String,
    pub iterations: usize,
    pub tool_calls: usize,
}

/// 会话信息
#[derive(Debug, Clone, Serialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub message_count: usize,
    pub token_estimate: usize,
}

/// 工具定义（注册用）
#[derive(Debug, Clone, Deserialize)]
pub struct RegisterToolRequest {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
    #[serde(default)]
    pub enabled: bool,
}

/// 全局配置（运行时可更新）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub llm: LlmConfig,
    pub gateway: GatewayConfig,
    pub memory: MemoryConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_host")]
    pub host: String,
}

fn default_port() -> u16 { 4848 }
fn default_host() -> String { "0.0.0.0".to_string() }

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            port: 4848,
            host: "0.0.0.0".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    #[serde(default = "default_working_window")]
    pub working_window: usize,
    #[serde(default = "default_data_dir")]
    pub data_dir: String,
}

fn default_working_window() -> usize { 64000 }
fn default_data_dir() -> String { "~/.pangu/data".to_string() }

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            working_window: 64000,
            data_dir: "~/.pangu/data".to_string(),
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            llm: LlmConfig::default(),
            gateway: GatewayConfig::default(),
            memory: MemoryConfig::default(),
        }
    }
}
