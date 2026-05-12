//! OpenAI API provider (GPT-4o, GPT-4o-mini, etc.)

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use crate::llm::provider::{LlmProvider, LlmMessage, LlmResponse, LlmToolCall, LlmTool};
use anyhow::Result;

const API_URL: &str = "https://api.openai.com/v1/chat/completions";

#[derive(Debug, Clone)]
pub struct OpenAiProvider {
    pub model: String,
    pub api_key: String,
    pub base_url: Option<String>,
    ctx_window: usize,
}

impl OpenAiProvider {
    pub fn new(model: &str, api_key: &str) -> Self {
        Self::with_base_url(model, api_key, API_URL)
    }

    pub fn with_base_url(model: &str, api_key: &str, base_url: &str) -> Self {
        let ctx_window = match model {
            m if m.contains("gpt-4o") => 128_000,
            m if m.contains("gpt-4-turbo") => 128_000,
            m if m.contains("gpt-4") => 8_192,
            m if m.contains("gpt-3.5-turbo") => 16_385,
            _ => 128_000,
        };
        Self {
            model: model.to_string(),
            api_key: api_key.to_string(),
            base_url: Some(base_url.to_string()),
            ctx_window,
        }
    }
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    tools: Option<Vec<ChatTool>>,
    temperature: f32,
    tool_choice: Option<String>,
}

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ToolCallSpec>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Serialize)]
struct ToolCallSpec {
    id: String,
    #[serde(rename = "type")]
    typ: String,
    function: ToolCallFunction,
}

#[derive(Serialize)]
struct ToolCallFunction {
    name: String,
    arguments: String,
}

#[derive(Serialize)]
struct ChatTool {
    #[serde(rename = "type")]
    typ: String,
    function: ToolFunction,
}

#[derive(Serialize)]
struct ToolFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Deserialize)]
struct ResponseMessage {
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<ResponseToolCall>>,
    #[serde(rename = " refusal_reasoning", default)]
    refusal_reasoning: Option<String>,
}

#[derive(Deserialize)]
struct ResponseToolCall {
    id: String,
    #[serde(rename = "type")]
    typ: String,
    function: ResponseFunction,
}

#[derive(Deserialize)]
struct ResponseFunction {
    name: String,
    arguments: String,
}

#[derive(Deserialize)]
struct Usage {
    #[serde(default)]
    prompt_tokens: Option<u32>,
    #[serde(default)]
    completion_tokens: Option<u32>,
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    async fn chat(
        &self,
        messages: Vec<LlmMessage>,
        tools: Option<Vec<LlmTool>>,
        temperature: f32,
    ) -> Result<LlmResponse> {
        let base = self.base_url.as_deref().unwrap_or(API_URL);
        let url = format!("{base}/chat/completions");

        let chat_messages: Vec<ChatMessage> = messages
            .into_iter()
            .map(|m| ChatMessage {
                role: m.role,
                content: m.content,
                tool_calls: None,
                tool_call_id: None,
            })
            .collect();

        let has_tools = tools.is_some();
        let chat_tools = tools.map(|tools| {
            tools.into_iter()
                .map(|t| ChatTool {
                    typ: "function".to_string(),
                    function: ToolFunction {
                        name: t.name,
                        description: t.description,
                        parameters: t.parameters,
                    },
                })
                .collect()
        });

        let req = ChatRequest {
            model: self.model.clone(),
            messages: chat_messages,
            tools: chat_tools,
            temperature,
            tool_choice: if has_tools { Some("auto".to_string()) } else { None },
        };

        let client = reqwest::Client::new();
        let resp = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&req)
            .send()
            .await?
            .error_for_status()?
            .json::<ChatResponse>()
            .await?;

        let choice = resp.choices.into_iter().next()
            .ok_or_else(|| anyhow::anyhow!("No choices in LLM response"))?;

        let tool_calls = choice
            .message
            .tool_calls
            .unwrap_or_default()
            .into_iter()
            .map(|tc| {
                let args: serde_json::Value = serde_json::from_str(&tc.function.arguments)
                    .unwrap_or(serde_json::Value::Object(Default::default()));
                LlmToolCall {
                    id: tc.id,
                    name: tc.function.name,
                    arguments: args,
                }
            })
            .collect();

        Ok(LlmResponse {
            content: choice.message.content,
            tool_calls,
            reasoning: None,
        })
    }

    fn name(&self) -> &str {
        &self.model
    }

    fn context_window(&self) -> usize {
        self.ctx_window
    }
}
