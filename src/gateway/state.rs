//! Gateway 全局状态

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::gateway::models::{AppConfig, LlmConfig};
use chrono::Utc;
use crate::llm::openai::OpenAiProvider;
use crate::llm::provider::LlmProvider;
use crate::memory::WorkingMemory;
use crate::tools::registry::ToolRegistry;
use crate::core::SessionId;

/// 应用全局状态
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<RwLock<AppConfig>>,
    /// 每个会话的 Agent 实例
    pub sessions: Arc<RwLock<HashMap<String, SessionHandle>>>,
    pub tools: ToolRegistry,
}

#[derive(Clone)]
pub struct SessionHandle {
    pub session_id: String,
    pub working_memory: WorkingMemory,
    pub llm: Arc<dyn LlmProvider>,
    pub tools: ToolRegistry,
    pub created_at: String,
}

impl AppState {
    pub fn new(config: AppConfig, tools: ToolRegistry) -> anyhow::Result<Self> {
        Ok(Self {
            config: Arc::new(RwLock::new(config)),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            tools,
        })
    }

    /// 从配置重建 LLM provider
    pub fn rebuild_llm(&self) -> anyhow::Result<Arc<dyn LlmProvider>> {
        let cfg = self.config.read().unwrap();
        let llm_cfg = &cfg.llm;
        
        let provider: Arc<dyn LlmProvider> = match llm_cfg.provider.as_str() {
            "openai" | "custom" => {
                Arc::new(OpenAiProvider::with_base_url(
                    &llm_cfg.model,
                    &llm_cfg.api_key,
                    &llm_cfg.base_url,
                ))
            }
            p => anyhow::bail!("不支持的 provider: {}", p),
        };
        
        Ok(provider)
    }

    /// 获取或创建会话
    pub fn get_or_create_session(&self) -> anyhow::Result<SessionHandle> {
        let cfg = self.config.read().unwrap();
        let memory_cfg = &cfg.memory;
        
        let working_window = memory_cfg.working_window;
        let working_memory = WorkingMemory::new(working_window);
        let llm = self.rebuild_llm()?;
        let tools = self.tools.clone();
        
        Ok(SessionHandle {
            session_id: SessionId::new().0,
            working_memory,
            llm,
            tools,
            created_at: Utc::now().to_rfc3339(),
        })
    }

    /// 更新 LLM 配置
    pub fn update_llm_config(&self, new_cfg: LlmConfig) -> anyhow::Result<()> {
        let mut cfg = self.config.write().unwrap();
        cfg.llm = new_cfg;
        Ok(())
    }

    /// 获取当前 LLM 配置（不脱敏）
    pub fn get_llm_config(&self) -> LlmConfig {
        let cfg = self.config.read().unwrap();
        cfg.llm.clone()
    }

    /// 更新全局配置
    pub fn update_config(&self, new_cfg: AppConfig) -> anyhow::Result<()> {
        let mut cfg = self.config.write().unwrap();
        *cfg = new_cfg;
        Ok(())
    }

    /// 注册工具
    pub async fn register_tool(&self, req: crate::gateway::models::RegisterToolRequest) -> anyhow::Result<()> {
        self.tools.register_dynamic(
            &req.name,
            &req.description,
            req.parameters,
        )?;
        Ok(())
    }

    /// 获取当前配置（脱敏 api_key）
    pub fn get_safe_config(&self) -> AppConfig {
        let cfg = self.config.read().unwrap();
        let mut c = (*cfg).clone();
        // 不暴露完整 api_key
        if !c.llm.api_key.is_empty() {
            c.llm.api_key = mask_key(&c.llm.api_key);
        }
        c
    }
}

fn mask_key(key: &str) -> String {
    if key.len() <= 8 {
        "****".to_string()
    } else {
        format!("{}...{}", &key[..4], &key[key.len()-4..])
    }
}
