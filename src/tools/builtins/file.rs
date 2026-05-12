//! 文件操作工具 - 读/写/编辑文件

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use crate::tools::registry::Tool;

#[derive(Serialize, Deserialize)]
struct ReadArgs {
    path: String,
    #[serde(default = "default_offset")]
    offset: usize,
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_offset() -> usize { 1 }
fn default_limit() -> usize { 500 }

#[derive(Serialize, Deserialize)]
struct WriteArgs {
    path: String,
    content: String,
}

#[derive(Serialize, Deserialize)]
struct EditArgs {
    path: String,
    old_string: String,
    new_string: String,
}

pub struct FileTools;

impl FileTools {
    pub fn new() -> Self { Self }
}

impl Default for FileTools {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl Tool for FileTools {
    fn name(&self) -> &str { "file" }
    fn description(&self) -> &str { "Read, write, or edit files on the filesystem" }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["read", "write", "edit"],
                    "description": "File operation to perform"
                },
                "path": {
                    "type": "string",
                    "description": "File path"
                },
                "content": { "type": "string", "description": "Content to write (for write/edit)" },
                "old_string": { "type": "string", "description": "String to replace (for edit)" },
                "new_string": { "type": "string", "description": "Replacement string (for edit)" },
                "offset": { "type": "integer", "description": "Line offset to start reading from" },
                "limit": { "type": "integer", "description": "Max lines to read" }
            },
            "required": ["action", "path"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<String> {
        let action = args["action"].as_str().unwrap_or("read");
        let path = args["path"].as_str().unwrap_or("");
        
        match action {
            "read" => {
                let content = tokio::fs::read_to_string(path).await
                    .map_err(|e| anyhow::anyhow!("Read error: {}", e))?;
                Ok(content)
            }
            "write" => {
                let content = args["content"].as_str().unwrap_or("");
                if let Some(parent) = std::path::Path::new(path).parent() {
                    tokio::fs::create_dir_all(parent).await?;
                }
                tokio::fs::write(path, content).await
                    .map_err(|e| anyhow::anyhow!("Write error: {}", e))?;
                Ok(format!("Written {} bytes to {}", content.len(), path))
            }
            "edit" => {
                let old_str = args["old_string"].as_str().unwrap_or("");
                let new_str = args["new_string"].as_str().unwrap_or("");
                let content = tokio::fs::read_to_string(path).await
                    .map_err(|e| anyhow::anyhow!("Read error: {}", e))?;
                if !content.contains(old_str) {
                    return Err(anyhow::anyhow!("old_string not found in file"));
                }
                let new_content = content.replace(old_str, new_str);
                tokio::fs::write(path, &new_content).await
                    .map_err(|e| anyhow::anyhow!("Write error: {}", e))?;
                Ok(format!("Edited {}", path))
            }
            _ => Err(anyhow::anyhow!("Unknown action: {}", action))
        }
    }
}
