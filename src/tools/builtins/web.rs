//! Web 工具 - 搜索和抓取网页

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use crate::tools::registry::Tool;

#[derive(Serialize, Deserialize)]
struct SearchArgs {
    query: String,
    #[serde(default = "default_count")]
    num_results: usize,
}

fn default_count() -> usize { 5 }

#[derive(Serialize, Deserialize)]
struct FetchArgs {
    url: String,
    #[serde(default)]
    extract_content: bool,
}

pub struct WebTools;

impl WebTools {
    pub fn new() -> Self { Self }
}

impl Default for WebTools {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl Tool for WebTools {
    fn name(&self) -> &str { "web" }
    fn description(&self) -> &str { "Search the web or fetch a URL's content" }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["search", "fetch"],
                    "description": "web search or fetch URL"
                },
                "query": { "type": "string", "description": "Search query (for search)" },
                "url": { "type": "string", "description": "URL to fetch (for fetch)" },
                "num_results": { "type": "integer", "description": "Number of results (for search)" },
                "extract_content": { "type": "boolean", "description": "Extract main content (for fetch)" }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<String> {
        let action = args["action"].as_str().unwrap_or("search");
        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (compatible; Pangu/1.0)")
            .timeout(std::time::Duration::from_secs(15))
            .build()?;

        match action {
            "search" => {
                let query = args["query"].as_str().unwrap_or("");
                let num = args["num_results"].as_u64().unwrap_or(5) as usize;
                // Simple web search using DuckDuckGo HTML
                let url = format!(
                    "https://html.duckduckgo.com/html/?q={}",
                    urlencoding::encode(query)
                );
                let resp = client.get(&url).send().await?.text().await?;
                // Extract snippet-like text from DDG HTML response
                let snippets: Vec<String> = resp.lines()
                    .filter(|l| l.contains("result__snippet"))
                    .take(num)
                    .map(|l| {
                        l.trim()
                            .replace("<b>", "")
                            .replace("</b>", "")
                            .replace("&amp;", "&")
                            .replace("&quot;", "\"")
                    })
                    .collect();
                Ok(snippets.join("\n\n"))
            }
            "fetch" => {
                let url = args["url"].as_str().unwrap_or("");
                let resp = client.get(url).send().await?;
                let text = resp.text().await?;
                Ok(format!("Fetched {} bytes from {}", text.len(), url))
            }
            _ => Err(anyhow::anyhow!("Unknown action: {}", action))
        }
    }
}

mod urlencoding {
    pub fn encode(s: &str) -> String {
        s.chars().map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            _ => format!("%{:02X}", c as u8),
        }).collect()
    }
}
