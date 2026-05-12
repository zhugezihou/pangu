//! 盘古 (Pangu) - 元Agent + Gateway 入口


use std::path::PathBuf;
use std::sync::Arc;
use anyhow::Result;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// CLI / Gateway 入口
#[derive(clap::ValueEnum, Clone, Debug)]
enum Mode {
    /// 直接运行单次任务
    Task,
    /// 启动 HTTP Gateway
    Gateway,
}

fn load_config_file() -> pangu_agent::gateway::AppConfig {
    let config_paths = [
        PathBuf::from("pangu.toml"),
        dirs::home_dir().unwrap_or_default().join(".pangu/pangu.toml"),
        PathBuf::from("/etc/pangu/pangu.toml"),
    ];

    for path in &config_paths {
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(path) {
                if let Ok(cfg) = toml::from_str(&content) {
                    tracing::info!("Loaded config from {}", path.display());
                    return cfg;
                }
            }
        }
    }
    
    tracing::warn!("No config file found, using defaults");
    pangu_agent::gateway::AppConfig::default()
}

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化 tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env()
            .add_directive("pangu=info".parse().unwrap()))
        .init();

    // CLI 参数解析
    let args: Vec<String> = std::env::args().collect();
    
    if args.contains(&"--help".to_string()) || args.contains(&"-h".to_string()) {
        println!(r#"盘古 (Pangu) v{} - 元Agent

Usage:
  pangu [OPTIONS]

Gateway Mode (默认):
  pangu                          # 启动 HTTP Gateway (默认端口 4848)
  pangu --port 9000              # 指定端口
  pangu --host 127.0.0.1         # 指定监听地址

Task Mode:
  pangu --task "你的任务"         # 单次执行任务（不启动 Gateway）

Config File (优先级从高到低):
  ./pangu.toml
  ~/.pangu/pangu.toml
  /etc/pangu/pangu.toml

pangu.toml 示例:
  [llm]
  provider = "openai"
  model = "gpt-4o-mini"
  api_key = "sk-..."
  base_url = "https://api.openai.com/v1"  # 可选，自定义端点
  temperature = 0.7

  [gateway]
  port = 4848
  host = "0.0.0.0"

  [memory]
  working_window = 64000
  data_dir = "~/.pangu/data"

Environment Variables (覆盖 config 文件):
  OPENAI_API_KEY, PANGU_API_KEY
  PANGU_MODEL (default: gpt-4o-mini)
  PANGU_PORT (default: 4848)
"#,
            env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    // 解析 --port / --host / --task
    let mut port: u16 = 4848;
    let mut host = "0.0.0.0".to_string();
    let mut task_mode = false;
    let mut task_text = String::new();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--port" | "-p" => { if let Some(v) = args.get(i+1) { port = v.parse().unwrap_or(4848); i += 2; } else { i += 1; } }
            "--host" => { if let Some(v) = args.get(i+1) { host = v.clone(); i += 2; } else { i += 1; } }
            "--task" | "-t" => { task_mode = true; if let Some(v) = args.get(i+1) { task_text = v.clone(); i += 2; } else { i += 1; } }
            _ => i += 1,
        }
    }

    // 加载配置
    let mut config = load_config_file();

    // 环境变量覆盖
    if let Ok(key) = std::env::var("OPENAI_API_KEY").or_else(|_| std::env::var("PANGU_API_KEY")) {
        config.llm.api_key = key;
    }
    if let Ok(model) = std::env::var("PANGU_MODEL") {
        config.llm.model = model;
    }
    if let Ok(port_str) = std::env::var("PANGU_PORT") {
        if let Ok(p) = port_str.parse() { port = p; }
    }

    if task_mode {
        // === Task Mode ===
        run_task(&config, &task_text).await
    } else {
        // === Gateway Mode ===
        run_gateway(&config, &host, port).await
    }
}

async fn run_task(config: &pangu_agent::gateway::AppConfig, task: &str) -> Result<()> {
    if config.llm.api_key.is_empty() {
        anyhow::bail!("OPENAI_API_KEY not set");
    }

    use pangu_agent::core::SessionId;
    use pangu_agent::llm::openai::OpenAiProvider;
    
    use pangu_agent::core::agent::Agent;
    use pangu_agent::tools::builtins::shell::ShellTool;
    use pangu_agent::tools::builtins::file::FileTools;
    use pangu_agent::tools::builtins::web::WebTools;
    use pangu_agent::tools::registry::ToolRegistry;
    use pangu_agent::memory::WorkingMemory;

    let llm = Arc::new(OpenAiProvider::with_base_url(
        &config.llm.model,
        &config.llm.api_key,
        &config.llm.base_url,
    ));

    let tools = ToolRegistry::new();
    tools.register(ShellTool::new());
    tools.register(FileTools::new());
    tools.register(WebTools::new());

    let session_id = SessionId::new();
    let working_memory = WorkingMemory::new(config.memory.working_window);

    tracing::info!("Session: {}", session_id.0);
    tracing::info!("Model: {}", config.llm.model);

    let mut agent = Agent::new(llm, tools, working_memory, &session_id.0);
    
    match agent.run(task).await {
        Ok(result) => {
            println!("\n=== Result ===\n{}", result);
        }
        Err(e) => {
            eprintln!("\n=== Error ===\n{}", e);
            std::process::exit(1);
        }
    }
    Ok(())
}

async fn run_gateway(config: &pangu_agent::gateway::AppConfig, host: &str, port: u16) -> Result<()> {
    
    use pangu_agent::gateway::{handlers, AppState};
    use pangu_agent::tools::builtins::shell::ShellTool;
    use pangu_agent::tools::builtins::file::FileTools;
    use pangu_agent::tools::builtins::web::WebTools;
    use pangu_agent::tools::registry::ToolRegistry;

    // 初始化工具注册表
    let tools = ToolRegistry::new();
    tools.register(ShellTool::new());
    tools.register(FileTools::new());
    tools.register(WebTools::new());

    // 创建应用状态
    let state = AppState::new(config.clone(), tools)?;

    // 构建路由
    let app = handlers::routes(state);

    let addr = format!("{}:{}", host, port);
    tracing::info!("Starting Pangu Gateway on http://{}", addr);
    tracing::info!("API docs: http://{}/docs (or see below)", addr);

    println!(r#"
╔══════════════════════════════════════════╗
║       盘古 (Pangu) Gateway v{}           ║
╠══════════════════════════════════════════╣
║  HTTP API: http://{}            ║
║                                          ║
║  POST /v1/run     - 执行任务              ║
║  GET  /v1/config  - 查看/修改配置         ║
║  GET  /v1/llm     - LLM 配置             ║
║  PUT  /v1/llm     - 更新 LLM 配置        ║
║  GET  /v1/sessions - 列出会话            ║
║  GET  /v1/tools   - 列出工具              ║
║  GET  /health    - 健康检查              ║
╚══════════════════════════════════════════╝
"#, env!("CARGO_PKG_VERSION"), addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
