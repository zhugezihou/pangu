//! Agent Core - ReAct (Reasoning + Acting) 主循环

use std::sync::Arc;
use std::time::{Duration, Instant};
use anyhow::Context;
use crate::core::{AgentResponse, Message, Role, ToolCall, ToolResult};
use crate::evolution::{ErrorCollector, SelfCorrector, SkillGenerator};
use crate::llm::provider::{LlmProvider, LlmTool, LlmMessage, LlmToolCall};
use crate::memory::WorkingMemory;
use crate::tools::registry::ToolRegistry;
use crate::tools::executor::ToolExecutor;

const MAX_ITERATIONS: usize = 50;
const DEFAULT_TEMPERATURE: f32 = 0.7;

pub struct Agent {
    llm: Arc<dyn LlmProvider>,
    tools: ToolRegistry,
    tool_executor: ToolExecutor,
    working_memory: WorkingMemory,
    error_collector: ErrorCollector,
    self_corrector: SelfCorrector,
    skill_generator: SkillGenerator,
    session_id: String,
    max_iterations: usize,
}

impl Agent {
    pub fn new(
        llm: Arc<dyn LlmProvider>,
        tools: ToolRegistry,
        working_memory: WorkingMemory,
        session_id: &str,
    ) -> Self {
        let tools_clone = tools.clone();
        let error_collector = ErrorCollector::new(None);
        Self {
            llm,
            tool_executor: ToolExecutor::new(tools),
            tools: tools_clone,
            working_memory,
            error_collector,
            self_corrector: SelfCorrector::new(),
            skill_generator: SkillGenerator::new(),
            session_id: session_id.to_string(),
            max_iterations: MAX_ITERATIONS,
        }
    }

    pub fn with_error_collector(mut self, ec: ErrorCollector) -> Self {
        self.error_collector = ec;
        self
    }

    pub fn with_max_iterations(mut self, max: usize) -> Self {
        self.max_iterations = max;
        self
    }

    /// 主循环：ReAct
    pub async fn run(&mut self, task: &str) -> anyhow::Result<String> {
        // 注入 system prompt
        let system_prompt = self.build_system_prompt();
        self.working_memory.push(Message::system(&system_prompt));
        self.working_memory.push(Message::user(task));

        for i in 0..self.max_iterations {
            tracing::info!("Iteration {}/{}", i + 1, self.max_iterations);

            // 压缩检查
            if self.working_memory.needs_compression() {
                tracing::info!("Compressing working memory");
                self.working_memory.compress(20);
            }

            // 调用 LLM
            let llm_messages = self.working_memory.to_llm_messages();
            let tool_defs = self.build_tool_defs();

            let resp = self.llm.chat(llm_messages, Some(tool_defs), DEFAULT_TEMPERATURE)
                .await
                .context("LLM call failed")?;

            // 解析响应
            if !resp.tool_calls.is_empty() {
                // 工具调用
                for tc in resp.tool_calls {
                    let call_id = tc.id.clone();
                    let tool_name = tc.name.clone();
                    let args = tc.arguments.clone();

                    // 记录 assistant 消息（包含思考）
                    if let Some(thought) = &resp.content {
                        self.working_memory.push(Message::assistant(thought));
                    }

                    self.working_memory.push(Message::assistant(&format!(
                        "Tool call: {} with args {}",
                        tool_name,
                        serde_json::to_string(&args).unwrap_or_default()
                    )));

                    // 执行工具
                    tracing::info!("Executing tool: {} (call_id={})", tool_name, call_id);
                    let result = self.tool_executor
                        .execute(&tool_name, args, call_id.clone())
                        .await;

                    // 记录工具结果
                    self.working_memory.push(Message::tool(&call_id, &result.output));

                    // 错误处理
                    if !result.success {
                        self.error_collector.record(
                            &self.session_id,
                            &format!("tool:{}", tool_name),
                            &format!("tool_error_{}", tool_name),
                            result.error.as_deref().unwrap_or("Unknown error"),
                            &result.output,
                        );
                    }
                }
            } else if let Some(content) = resp.content {
                // 文本响应（可能包含思考过程）
                let content = content.trim();
                if content.to_lowercase().contains("任务完成") 
                    || content.to_lowercase().contains("done")
                    || content.to_lowercase().contains("完成") {
                    self.working_memory.push(Message::assistant(content));
                    return Ok(content.to_string());
                }
                self.working_memory.push(Message::assistant(content));
                // 检查是否在思考而非结束
                if i == self.max_iterations - 1 {
                    return Ok(content.to_string());
                }
            } else {
                return Err(anyhow::anyhow!("Empty LLM response"));
            }
        }

        Err(anyhow::anyhow!("Max iterations ({}) reached", self.max_iterations))
    }

    fn build_system_prompt(&self) -> String {
        let enabled_tools = self.tools.list_enabled();
        let tool_names: Vec<&str> = enabled_tools.iter().map(|t| t.name.as_str()).collect();
        format!(
            r#"你是盘古，一个能真正干活的AI Agent。

核心原则：
1. 直接执行任务，不废话
2. 每个行动都要有明确目的
3. 工具执行后必须评估结果

可用工具：{}

每次行动遵循 ReAct 循环：
1. Think: 分析当前状态，决定下一步
2. Act: 调用工具（如果需要）
3. Observe: 评估工具结果
4. Adapt: 成功则继续，失败则调整策略

当任务完成时，明确回复"任务完成"或"Done"。"#,
            tool_names.join(", ")
        )
    }

    fn build_tool_defs(&self) -> Vec<LlmTool> {
        self.tools.list_enabled()
            .iter()
            .map(|t| LlmTool {
                name: t.name.clone(),
                description: t.description.clone(),
                parameters: t.parameters.clone(),
            })
            .collect()
    }
}
