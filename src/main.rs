//! 盘古 (Pangu) - 元Agent入口

use std::sync::Arc;
use anyhow::Result;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use pangu::core::SessionId;
use pangu::llm::openai::OpenAiProvider;
use pangu::llm::provider::LlmProvider;
use pangu::core::agent::Agent;
use pangu::tools::builtins::shell::ShellTool;
use pangu::tools::builtins::file::FileTools;
use pangu::tools::builtins::web::WebTools;
use pangu::tools::registry::ToolRegistry;
use pangu::memory::WorkingMemory;
use dirs::home_dir;

fn init_tracing() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env()
            .add_directive("pangu=info".parse().unwrap()))
        .init();
}

fn load_config() -> (String, String, usize) {
    let model = std::env::var("PANGU_MODEL")
        .unwrap_or_else(|_| "gpt-4o-mini".to_string());
    let api_key = std::env::var("OPENAI_API_KEY")
        .or_else(|_| std::env::var("PANGU_API_KEY"))
        .unwrap_or_default();
    let working_window: usize = std::env::var("PANGU_WINDOW")
        .unwrap_or_else(|_| "64000".to_string())
        .parse()
        .unwrap_or(64000);
    (model, api_key, working_window)
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let args: Vec<String> = std::env::args().collect();
    
    if args.contains(&"--help".to_string()) || args.contains(&"-h".to_string()) {
        println!(r#"盘古 (Pangu) - 元Agent

Usage:
  pangu                    # Interactive mode
  pangu --task "..."       # Single task mode
  pangu --skills           # List generated skills
  
Environment:
  OPENAI_API_KEY           # OpenAI API key
  PANGU_MODEL              # Model name (default: gpt-4o-mini)
  PANGU_WINDOW             # Working memory window (default: 64000)
"#);
        return Ok(());
    }

    let (model, api_key, working_window) = load_config();

    if api_key.is_empty() {
        eprintln!("Error: OPENAI_API_KEY or PANGU_API_KEY not set");
        std::process::exit(1);
    }

    // 初始化 LLM
    let llm = Arc::new(OpenAiProvider::new(&model, &api_key));
    tracing::info!("LLM: {} (context: {})", llm.name(), llm.context_window());

    // 初始化工具注册表
    let tools = ToolRegistry::new();
    tools.register(ShellTool::new());
    tools.register(FileTools::new());
    tools.register(WebTools::new());
    tracing::info!("Registered tools: {:?}", tools.list().into_iter().map(|t| t.name).collect::<Vec<_>>());

    // 数据目录
    let data_dir = home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".pangu/data");

    std::fs::create_dir_all(&data_dir)?;

    // 创建会话
    let session_id = SessionId::new();
    let working_memory = WorkingMemory::new(working_window);

    tracing::info!("Session: {}", session_id.0);

    let mut agent = Agent::new(llm, tools, working_memory, &session_id.0);

    if let Some(idx) = args.iter().position(|a| a == "--task" || a == "-t") {
        if let Some(task) = args.get(idx + 1) {
            tracing::info!("Executing task: {}", task);
            match agent.run(task).await {
                Ok(result) => {
                    println!("\n=== Result ===\n{}", result);
                }
                Err(e) => {
                    eprintln!("\n=== Error ===\n{}", e);
                    std::process::exit(1);
                }
            }
        }
    } else {
        println!("盘古 (Pangu) - 元Agent");
        println!("Session: {}", session_id.0);
        println!("Model: {}", model);
        println!("Type a task or press Ctrl+C to exit.\n");
        println!("(Interactive mode not yet implemented — use --task)");
        println!("Example: pangu --task \"帮我查一下今天的天气\"");
    }

    Ok(())
}
