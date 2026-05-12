//! Working Memory - 当前会话上下文，随对话动态增长

use crate::core::{Message, MemoryEntry, MemoryType};
use crate::llm::provider::LlmMessage;

pub struct WorkingMemory {
    messages: Vec<Message>,
    max_tokens: usize,
}

impl WorkingMemory {
    pub fn new(max_tokens: usize) -> Self {
        Self { messages: vec![], max_tokens }
    }

    pub fn push(&mut self, msg: Message) {
        self.messages.push(msg);
    }

    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    pub fn messages_mut(&mut self) -> &mut Vec<Message> {
        &mut self.messages
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// 估算当前 token 数（粗略：每token约4字符）
    pub fn estimate_tokens(&self) -> usize {
        self.messages.iter()
            .map(|m| m.content.len())
            .sum::<usize>() / 4
    }

    pub fn needs_compression(&self) -> bool {
        self.estimate_tokens() > self.max_tokens * 70 / 100
    }

    /// 压缩：保留最近 N 条消息 + system prompt
    pub fn compress(&mut self, keep_recent: usize) {
        if self.messages.len() <= keep_recent {
            return;
        }
        // 保留 system 消息（第一条） + 最近的消息
        let system_msg = self.messages.first().cloned();
        let rest: Vec<_> = self.messages.drain(keep_recent..).collect();
        let kept: Vec<_> = self.messages.drain(1..).rev().take(keep_recent - 1).collect();
        
        self.messages.clear();
        if let Some(sys) = system_msg {
            self.messages.push(sys);
        }
        self.messages.extend(kept.into_iter().rev());
        // 注入压缩说明
        self.messages.push(Message::assistant(
            "[Previous context compressed. Summary of dropped messages not shown.]"
        ));
    }

    pub fn to_llm_messages(&self) -> Vec<LlmMessage> {
        self.messages.iter().map(|m| LlmMessage {
            role: match m.role {
                crate::core::Role::System => "system".to_string(),
                crate::core::Role::User => "user".to_string(),
                crate::core::Role::Assistant => "assistant".to_string(),
                crate::core::Role::Tool => "tool".to_string(),
            },
            content: m.content.clone(),
        }).collect()
    }
}
