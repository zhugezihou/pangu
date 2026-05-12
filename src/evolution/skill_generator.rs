//! Skill Generator - 用 LLM 生成新的工具代码

use crate::core::Skill;
use crate::evolution::self_corrector::CorrectionStrategy;
use crate::llm::provider::LlmProvider;
use anyhow::Result;

pub struct SkillGenerator;

impl SkillGenerator {
    pub fn new() -> Self { Self }

    /// 根据策略生成技能
    pub async fn generate(
        &self,
        llm: &dyn LlmProvider,
        strategy: &CorrectionStrategy,
        task_context: &str,
    ) -> Result<Option<Skill>> {
        let prompt = match strategy {
            CorrectionStrategy::CreateTool { tool_name, reason } => {
                format!(
                    r#"Task context:
{task_context}

Generate a Rust tool implementation for "{tool_name}" ({reason}).
The tool should implement the Tool trait from crate `crate::tools::registry::Tool`.

Requirements:
1. Use `#[async_trait]`
2. Return `anyhow::Result<String>`
3. Parameters via `serde_json::Value`
4. Include proper error handling
5. Output ONLY the Rust code in a code block

Example signature:
```rust
use async_trait::async_trait;
use serde::{{Deserialize, Serialize}};
use crate::tools::registry::Tool;

pub struct {}Tool;

#[async_trait]
impl Tool for {}Tool {{
    fn name(&self) -> &str {{ "{}" }}
    fn description(&self) -> &str {{ "..." }}
    fn parameters(&self) -> serde_json::Value {{ ... }}
    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<String> {{
        // implementation
    }}
}}
```"#,
                    tool_name, tool_name, tool_name
                )
            }
            _ => return Ok(None),
        };

        let messages = vec![
            crate::llm::provider::LlmMessage {
                role: "system".to_string(),
                content: "You are a skilled Rust programmer. Generate production-quality tool implementations.".to_string(),
            },
            crate::llm::provider::LlmMessage {
                role: "user".to_string(),
                content: prompt.clone(),
            },
        ];

        let resp = llm.chat(messages, None, 0.3).await?;
        let code = match resp.content {
            Some(c) => c,
            None => return Ok(None),
        };

        let skill = Skill::new(
            &format!("{}Tool", match strategy {
                CorrectionStrategy::CreateTool { tool_name, .. } => tool_name,
                _ => "custom",
            }),
            task_context,
            &code,
            match strategy {
                CorrectionStrategy::CreateTool { tool_name, .. } => tool_name,
                _ => "custom",
            },
            serde_json::json!({}),
        );

        Ok(Some(skill))
    }
}
