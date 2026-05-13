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
use async_stream::try_stream;
use axum::response::sse::{Event, Sse};
use futures::Stream;
use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use std::convert::Infallible;
use std::pin::Pin;

use crate::core::StepEvent;
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
        .route("/v1/run_stream", post(run_task_stream))
        .route("/v1/debug_sse", get(debug_sse_stream))
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
/// Agent is created INSIDE the std thread so no !Send crossing occurs.
fn run_agent_sync(
    llm: Arc<dyn LlmProvider>,
    tools: ToolRegistry,
    working_memory: WorkingMemory,
    session_id: String,
    task: String,
    max_iterations: Option<usize>,
) -> RunResponse {
    // Agent::new does NOT create rusqlite::Connection (ErrorCollector::new(None)).
    // Handle::current().block_on() works here because std::thread has no active runtime.
    // Create agent INSIDE the std thread. The builder creates a fresh runtime
    // that is completely independent of any outer runtime, avoiding deadlock.
    let session_id_for_agent = session_id.clone();
    let result = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to create runtime");
        let mut agent = crate::core::agent::Agent::new(
            llm,
            tools,
            working_memory,
            &session_id_for_agent,
        );
        if let Some(max_iters) = max_iterations {
            agent = agent.with_max_iterations(max_iters);
        }
        rt.block_on(agent.run(&task))
    }).join().unwrap_or_else(|e| {
        Err(anyhow::anyhow!("thread panicked: {:?}", e))
    });

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

/// Helper: run the agent with step events via broadcast channel (sync, called via spawn_blocking)
fn run_agent_sync_with_steps(
    llm: Arc<dyn LlmProvider>,
    tools: ToolRegistry,
    working_memory: WorkingMemory,
    session_id: String,
    task: String,
    max_iterations: Option<usize>,
    step_tx: broadcast::Sender<StepEvent>,
) -> RunResponse {
    let session_id_for_agent = session_id.clone();
    let result = std::thread::spawn(move || {
        let mut agent = crate::core::agent::Agent::new(
            llm,
            tools,
            working_memory,
            &session_id_for_agent,
        );
        if let Some(max_iters) = max_iterations {
            agent = agent.with_max_iterations(max_iters);
        }
        agent = agent.with_step_sender(step_tx);
        // Fresh independent runtime avoids deadlock with any outer runtime.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to create runtime");
        rt.block_on(agent.run(&task))
    }).join().unwrap_or_else(|e| {
        Err(anyhow::anyhow!("thread panicked: {:?}", e))
    });

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

/// GET /v1/debug_sse - Debug SSE with fake events (no agent)
async fn debug_sse_stream() -> Sse<Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>> {
    let (step_tx, step_rx) = broadcast::channel::<StepEvent>(100);

    // Spawn thread to send test events
    let _handle = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(100));
        let _ = step_tx.send(StepEvent::Thinking("debug: event 1".to_string()));
        std::thread::sleep(std::time::Duration::from_millis(200));
        let _ = step_tx.send(StepEvent::Thinking("debug: event 2".to_string()));
        std::thread::sleep(std::time::Duration::from_millis(200));
        let _ = step_tx.send(StepEvent::Done {
            result: "debug done".to_string(),
            iterations: 2,
            tool_calls: 0,
        });
    });

    let stream = async_stream::stream! {
        let mut rx = step_rx;
        loop {
            match rx.recv().await {
                Ok(step) => {
                    let (event_name, data) = match step {
                        StepEvent::Thinking(text) => ("thinking", text),
                        StepEvent::CallingTool { name, call_id: _ } => ("tool_call", name),
                        StepEvent::ToolResult { name, success } => {
                            ("tool_result", format!("{}: {}", name, if success { "ok" } else { "failed" }))
                        }
                        StepEvent::Done { result, iterations: _, tool_calls: _ } => ("done", result),
                    };
                    let event = Event::default().event(event_name).data(data);
                    yield event;
                }
                Err(broadcast::error::RecvError::Closed) => break,
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("SSE broadcast lagged, skipping {} events", n);
                    continue;
                }
            }
        }
    };

    let boxed = Box::pin(stream.map(Ok)) as Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>;
    Sse::new(boxed).keep_alive(axum::response::sse::KeepAlive::default())
}

/// POST /v1/run_stream - Run agent with SSE streaming of step events
/// POST /v1/run_stream - Run agent with SSE streaming of step events
async fn run_task_stream(
    State(state): State<AppState>,
    Json(req): Json<RunRequest>,
) -> Sse<Pin<Box<dyn tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>> + Send>>> {
    // Create broadcast channel for step events
    let (step_tx, step_rx) = broadcast::channel::<StepEvent>(100);

    // Spawn agent in background using std::thread (not spawn_blocking) to avoid
    // blocking the async executor. std::thread can hold !Send types like Agent.
    // The thread calls get_or_create_session() which creates a NEW session internally.
    let state_for_thread = state.clone();
    let task_text = req.task.clone();
    let max_iters = req.max_iterations;
    let step_tx_for_agent = step_tx.clone();
    let _handle = std::thread::spawn(move || {
        // Create a fresh tokio runtime for the agent
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to create runtime");

        // get_or_create_session creates a NEW session with its own session_id.
        // The returned SessionHandle's session_id is the authoritative one.
        let session = match state_for_thread.get_or_create_session() {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("SSE: session create failed: {}", e);
                let _ = step_tx_for_agent.send(StepEvent::Done {
                    result: format!("session error: {}", e),
                    iterations: 0,
                    tool_calls: 0,
                });
                return;
            }
        };
        let session_id = session.session_id.clone();
        let (llm, tools, working_memory) = (session.llm.clone(), session.tools.clone(), session.working_memory.clone());

        let mut agent = crate::core::agent::Agent::new(
            llm,
            tools,
            working_memory,
            &session_id,
        );
        if let Some(max_iters) = max_iters {
            agent = agent.with_max_iterations(max_iters);
        }
        agent = agent.with_step_sender(step_tx_for_agent.clone());

        let result = rt.block_on(agent.run(&task_text));
        // Send final Done with real stats
        let done_info: (String, usize, usize) = match result {
            Ok(r) => (r, agent.current_step, agent.tool_call_count),
            Err(e) => (format!("error: {}", e), 0, 0),
        };
        let _ = step_tx_for_agent.send(StepEvent::Done {
            result: done_info.0,
            iterations: done_info.1,
            tool_calls: done_info.2,
        });
    });

    // Return SSE stream using async-stream
    let step_rx = step_tx.subscribe();
    let stream = async_stream::stream! {
        let mut rx = step_rx;
        loop {
            match rx.recv().await {
                Ok(step) => {
                    let (event_name, data) = match step {
                        StepEvent::Thinking(text) => ("thinking", text),
                        StepEvent::CallingTool { name, call_id: _ } => ("tool_call", name),
                        StepEvent::ToolResult { name, success } => {
                            ("tool_result", format!("{}: {}", name, if success { "ok" } else { "failed" }))
                        }
                        StepEvent::Done { result, iterations: _, tool_calls: _ } => ("done", result),
                    };
                    let event = Event::default().event(event_name).data(data);
                    yield event;
                }
                Err(broadcast::error::RecvError::Closed) => break,
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("SSE broadcast lagged, skipping {} events", n);
                    continue;
                }
            }
        }
    };

    // AsyncStream<Event, _> → Stream<Item = Result<Event, Infallible>>
    let stream = stream.map(Ok);
    let boxed = Box::pin(stream) as Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>;
    Sse::new(boxed)
        .keep_alive(axum::response::sse::KeepAlive::default())
}
