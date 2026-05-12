//! Gateway HTTP Handlers - axum 0.7
//! 所有 handler 用 Json 响应 + 标准 Error 类型

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};

use crate::gateway::{
    models::{AppConfig, LlmConfig, RegisterToolRequest, RunRequest, RunResponse, SessionInfo},
    state::AppState,
};
use crate::llm::provider::LlmProvider;
use crate::memory::WorkingMemory;
use crate::tools::registry::ToolRegistry;

// ============ 路由 ============

pub fn routes(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/config", get(get_config).put(update_config))
        .route("/v1/llm", get(get_llm_config).put(update_llm_config))
        .route("/v1/run", post(run_task))
        .route("/v1/sessions", get(list_sessions))
        .route("/v1/sessions/:id", get(get_session))
        .route("/v1/tools", get(list_tools).post(register_tool))
        .with_state(state)
}

// ============ Handlers ============

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "service": "pangu-gateway",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

async fn get_config(State(state): State<AppState>) -> Json<AppConfig> {
    Json(state.get_safe_config())
}

async fn update_config(
    State(state): State<AppState>,
    Json(cfg): Json<AppConfig>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    state.update_config(cfg).map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(Json(serde_json::json!( {"status": "ok"} )))
}

async fn get_llm_config(State(state): State<AppState>) -> Json<LlmConfig> {
    let mut cfg = state.get_llm_config();
    if !cfg.api_key.is_empty() {
        let len = cfg.api_key.len();
        cfg.api_key = if len <= 8 {
            "********".to_string()
        } else {
            format!("{}...{}", &cfg.api_key[..4], &cfg.api_key[len - 4..])
        };
    }
    Json(cfg)
}

async fn update_llm_config(
    State(state): State<AppState>,
    Json(cfg): Json<LlmConfig>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    state.update_llm_config(cfg).map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(Json(serde_json::json!( {"status": "ok"} )))
}

/// Helper: run the agent (sync, called via spawn_blocking)
fn run_agent_sync(
    llm: Arc<dyn LlmProvider>,
    tools: ToolRegistry,
    working_memory: WorkingMemory,
    session_id: String,
    task: String,
    max_iterations: Option<usize>,
) -> RunResponse {
    let mut agent = crate::core::agent::Agent::new(
        llm,
        tools,
        working_memory,
        &session_id,
    );
    if let Some(max_iters) = max_iterations {
        agent = agent.with_max_iterations(max_iters);
    }

    // Use tokio's blocking thread pool to run the async agent
    let rt = tokio::runtime::Handle::current();
    let result = rt.block_on(agent.run(&task));

    match result {
        Ok(result) => RunResponse {
            session_id,
            result,
            iterations: 1,
            tool_calls: 0,
        },
        Err(e) => {
            tracing::error!("Agent run failed: {}", e);
            RunResponse {
                session_id: String::new(),
                result: format!("agent error: {}", e),
                iterations: 0,
                tool_calls: 0,
            }
        }
    }
}

/// POST /v1/run - Run the agent
async fn run_task(
    State(state): State<AppState>,
    Json(req): Json<RunRequest>,
) -> Json<RunResponse> {
    let session_id = req.session_id.clone().unwrap_or_else(|| crate::core::SessionId::new().0);

    // Get or create session
    let (llm, tools, working_memory) = {
        let mut sessions = state.sessions.write().unwrap();
        if !sessions.contains_key(&session_id) {
            match state.get_or_create_session() {
                Ok(handle) => { sessions.insert(session_id.clone(), handle); }
                Err(e) => {
                    return Json(RunResponse {
                        session_id: String::new(),
                        result: format!("session create failed: {}", e),
                        iterations: 0,
                        tool_calls: 0,
                    });
                }
            }
        }
        let session = sessions.get(&session_id).cloned();
        match session {
            Some(s) => (s.llm.clone(), s.tools.clone(), s.working_memory.clone()),
            None => {
                return Json(RunResponse {
                    session_id: String::new(),
                    result: "session not found".to_string(),
                    iterations: 0,
                    tool_calls: 0,
                });
            }
        }
    };

    let task_text = req.task.clone();
    let max_iters = req.max_iterations;

    // Run in blocking thread so non-Send Agent doesn't infect the async future
    let result = tokio::task::spawn_blocking(move || {
        run_agent_sync(llm, tools, working_memory, session_id, task_text, max_iters)
    })
    .await
    .unwrap_or_else(|e| RunResponse {
        session_id: String::new(),
        result: format!("task join error: {}", e),
        iterations: 0,
        tool_calls: 0,
    });

    Json(result)
}

async fn list_sessions(State(state): State<AppState>) -> Json<Vec<SessionInfo>> {
    let sessions = state.sessions.read().unwrap();
    let infos: Vec<SessionInfo> = sessions
        .values()
        .map(|s| SessionInfo {
            session_id: s.session_id.clone(),
            message_count: s.working_memory.len(),
            token_estimate: s.working_memory.estimate_tokens(),
        })
        .collect();
    Json(infos)
}

async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SessionInfo>, StatusCode> {
    let sessions = state.sessions.read().unwrap();
    match sessions.get(&id) {
        Some(s) => Ok(Json(SessionInfo {
            session_id: s.session_id.clone(),
            message_count: s.working_memory.len(),
            token_estimate: s.working_memory.estimate_tokens(),
        })),
        None => Err(StatusCode::NOT_FOUND),
    }
}

async fn list_tools(State(state): State<AppState>) -> Json<serde_json::Value> {
    let tools = state.tools.list();
    Json(serde_json::json!({"tools": tools, "count": tools.len()}))
}

async fn register_tool(
    State(state): State<AppState>,
    Json(req): Json<RegisterToolRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    state
        .register_tool(req)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(Json(serde_json::json!( {"status": "ok"} )))
}
