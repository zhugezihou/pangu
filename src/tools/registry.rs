//! 工具注册表 - 动态注册和管理工具

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
    pub enabled: bool,
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> serde_json::Value;
    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<String>;
}

pub struct ToolRegistry {
    tools: Arc<RwLock<HashMap<String, Arc<dyn Tool>>>>,
    defs: Arc<RwLock<HashMap<String, ToolDef>>>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for ToolRegistry {
    fn clone(&self) -> Self {
        Self {
            tools: self.tools.clone(),
            defs: self.defs.clone(),
        }
    }
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: Arc::new(RwLock::new(HashMap::new())),
            defs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn register<T: Tool + 'static>(&self, tool: T) {
        let name = tool.name().to_string();
        let def = ToolDef {
            name: tool.name().to_string(),
            description: tool.description().to_string(),
            parameters: tool.parameters(),
            enabled: true,
        };
        let dyn_tool: Arc<dyn Tool> = Arc::new(tool);
        self.tools.write().unwrap().insert(name.clone(), dyn_tool);
        self.defs.write().unwrap().insert(name, def);
    }

    /// 动态注册工具（通过 JSON Schema，无须实现 Tool trait）
    pub fn register_dynamic(&self, name: &str, description: &str, parameters: serde_json::Value) -> anyhow::Result<()> {
        let def = ToolDef {
            name: name.to_string(),
            description: description.to_string(),
            parameters,
            enabled: true,
        };
        self.defs.write().unwrap().insert(name.to_string(), def);
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.read().unwrap().get(name).cloned()
    }

    pub fn list(&self) -> Vec<ToolDef> {
        self.defs.read().unwrap().values().cloned().collect()
    }

    pub fn list_enabled(&self) -> Vec<ToolDef> {
        self.defs.read().unwrap().values()
            .filter(|d| d.enabled)
            .cloned()
            .collect()
    }

    pub fn enable(&self, name: &str) {
        if let Some(d) = self.defs.write().unwrap().get_mut(name) {
            d.enabled = true;
        }
    }

    pub fn disable(&self, name: &str) {
        if let Some(d) = self.defs.write().unwrap().get_mut(name) {
            d.enabled = false;
        }
    }

    pub fn unregister(&self, name: &str) {
        self.tools.write().unwrap().remove(name);
        self.defs.write().unwrap().remove(name);
    }
}
