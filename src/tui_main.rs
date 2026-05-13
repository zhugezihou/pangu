//! Pangu TUI - Interactive terminal UI for Pangu Gateway

use anyhow::Result;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Terminal,
};
use std::io;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

// ============ Models ============

#[derive(Debug, Clone, serde::Deserialize)]
pub struct RunResponse {
    pub session_id: String,
    pub result: String,
    pub iterations: usize,
    pub tool_calls: usize,
}

#[derive(Debug, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
    pub meta: Option<String>,
}

// ============ App State ============

#[derive(Debug)]
pub struct PendingRequest {
    pub response_rx: Receiver<Result<RunResponse, String>>,
    pub status_tx: Sender<String>,
    pub status_rx: Receiver<String>,
}

#[derive(Debug, Clone)]
pub enum AppRequest {
    Send {
        task: String,
        session_id: Option<String>,
        response_tx: Sender<Result<RunResponse, String>>,
        status_tx: Sender<String>,
    },
}

pub struct App {
    pub messages: Vec<Message>,
    pub input: String,
    pub session_id: Option<String>,
    pub base_url: String,
    pub running: bool,
    pub waiting: bool,
    pub waiting_dots: u8,
    pub pending_request: Option<PendingRequest>,
}

impl App {
    fn new(base_url: String) -> Self {
        Self {
            messages: Vec::new(),
            input: String::new(),
            session_id: None,
            base_url,
            running: true,
            waiting: false,
            waiting_dots: 0,
            pending_request: None,
        }
    }

    fn add_user(&mut self, text: String) {
        self.messages.push(Message {
            role: "user".into(),
            content: text,
            meta: None,
        });
    }

    fn add_assistant(&mut self, content: String, meta: Option<String>) {
        self.messages.push(Message {
            role: "assistant".into(),
            content,
            meta,
        });
    }

    fn add_system(&mut self, text: String) {
        self.messages.push(Message {
            role: "system".into(),
            content: text,
            meta: None,
        });
    }

    fn add_error(&mut self, text: String) {
        self.messages.push(Message {
            role: "error".into(),
            content: text,
            meta: None,
        });
    }
}

// ============ HTTP Client (runs in dedicated thread) ============

fn http_worker(base_url: String, rx: Receiver<AppRequest>) {
    let client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to build HTTP client: {}", e);
            return;
        }
    };

    while let Ok(req) = rx.recv() {
        match req {
            AppRequest::Send {
                task,
                session_id,
                response_tx,
                status_tx,
            } => {
                // Announce thinking
                let _ = status_tx.send("🤔 Thinking...".into());

                let mut json_req = serde_json::json!({ "task": task });
                if let Some(ref sid) = session_id {
                    json_req["session_id"] = serde_json::json!(sid);
                }

                // Send status updates while waiting
                let _ = status_tx.send("📡 Calling LLM...".into());

                let result: Result<RunResponse, String> = client
                    .post(format!("{}/v1/run", base_url))
                    .json(&json_req)
                    .send()
                    .map_err(|e| e.to_string())
                    .and_then(|resp| {
                        let status = resp.status();
                        if status.is_success() {
                            resp.json::<RunResponse>().map_err(|e| e.to_string())
                        } else {
                            let body = resp.text().unwrap_or_default();
                            Err(format!("API error {}: {}", status, body))
                        }
                    });

                match &result {
                    Ok(_) => {
                        let _ = status_tx.send("✅ Done!".into());
                    }
                    Err(_) => {
                        let _ = status_tx.send("❌ Failed".into());
                    }
                }
                let _ = response_tx.send(result);
            }
        }
    }
}

// ============ TUI Drawing ============

fn draw(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &App) -> Result<()> {
    terminal.draw(|f| {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Length(3),
            ])
            .split(f.area());

        // Title
        let title = Paragraph::new("Pangu TUI - AI Agent")
            .alignment(Alignment::Center)
            .style(Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD));
        f.render_widget(title, chunks[0]);

        // Messages
        let msgs: Vec<Line> = app
            .messages
            .iter()
            .map(|m| {
                let style = match m.role.as_str() {
                    "user" => Style::new().fg(Color::Green),
                    "assistant" => Style::new().fg(Color::Yellow),
                    "error" => Style::new().fg(Color::Red),
                    _ => Style::new().fg(Color::Blue),
                };
                let mut line = Line::from(vec![Span::raw(&m.content)]);
                if let Some(ref meta) = m.meta {
                    line.spans.push(ratatui::text::Span::raw(format!(" [{}]", meta)));
                }
                Line::from(line.spans.into_iter().map(|s| s.style(style)).collect::<Vec<_>>())
            })
            .collect();

        let scroll = if msgs.len() > 20 { msgs.len() - 20 } else { 0 };
        let list = Paragraph::new(msgs)
            .wrap(Wrap { trim: false })
            .scroll((scroll as u16, 0));
        f.render_widget(list, chunks[1]);

        // Input
        let input_text = if app.input.is_empty() {
            "> ".to_string()
        } else {
            format!("> {}", app.input)
        };
        let input = Paragraph::new(input_text)
            .style(Style::new().fg(Color::White).bg(Color::Black))
            .block(
                Block::default()
                    .title(" Message ")
                    .borders(Borders::ALL)
                    .border_style(Style::new().fg(Color::White)),
            );
        f.render_widget(input, chunks[2]);

        // Cursor
        let cursor_x = (app.input.len() + 2).min(chunks[2].width as usize - 3);
        let pos = ratatui::layout::Position::new(
            chunks[2].x + cursor_x as u16,
            chunks[2].y + 1,
        );
        f.set_cursor_position(pos);
    })?;
    Ok(())
}

// ============ Input Handling ============

fn handle_key(app: &mut App, key: crossterm::event::KeyEvent, req_tx: &Sender<AppRequest>) {
    use crossterm::event::KeyCode;

    if app.waiting {
        return;
    }

    match key.code {
        KeyCode::Char(c)
            if !key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL) =>
        {
            app.input.push(c);
        }
        KeyCode::Backspace => {
            app.input.pop();
        }
        KeyCode::Enter => {
            let input = std::mem::take(&mut app.input);
            if input.is_empty() {
                return;
            }

            // Commands
            match input.trim() {
                "/quit" | "/q" => {
                    app.running = false;
                    return;
                }
                "/clear" => {
                    app.messages.clear();
                    return;
                }
                "/session" => {
                    if let Some(ref sid) = app.session_id {
                        app.add_system(format!("Session ID: {}", sid));
                    } else {
                        app.add_system("No active session".to_string());
                    }
                    return;
                }
                s if s.starts_with('/') => {
                    app.add_error(format!("Unknown command: {}", s));
                    return;
                }
                _ => {}
            }

            // Show user message immediately
            app.add_user(input.clone());

            // Send to HTTP worker (non-blocking via channel)
            let (response_tx, response_rx): (
                Sender<Result<RunResponse, String>>,
                Receiver<Result<RunResponse, String>>,
            ) = channel();
            let (status_tx, status_rx): (Sender<String>, Receiver<String>) = channel();
            let session_id = app.session_id.clone();
            let _ = req_tx.send(AppRequest::Send {
                task: input,
                session_id,
                response_tx,
                status_tx: status_tx.clone(),
            });

            app.waiting = true;
            app.pending_request = Some(PendingRequest {
                response_rx,
                status_tx,
                status_rx,
            });
        }
        KeyCode::Esc => {
            app.input.clear();
        }
        _ => {}
    }
}

// ============ Main ============

fn main() -> Result<()> {
    // Init terminal
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Gateway URL
    let base_url =
        std::env::var("PANGU_BASE_URL").unwrap_or_else(|_| "http://localhost:4848".to_string());

    let mut app = App::new(base_url.clone());

    // Check gateway connectivity
    match reqwest::blocking::Client::new()
        .get(format!("{}/health", app.base_url))
        .timeout(std::time::Duration::from_secs(3))
        .send()
    {
        Ok(resp) if resp.status().is_success() => {
            app.add_system("Welcome to Pangu TUI! Type a message and press Enter.".to_string());
            app.add_system("Commands: /quit /clear /session".to_string());
        }
        _ => {
            app.add_error(
                "Gateway unreachable. Start with: cargo run --bin pangu-agent".to_string(),
            );
        }
    }

    // Spawn HTTP worker thread
    let (req_tx, req_rx): (Sender<AppRequest>, _) = channel();
    thread::spawn(move || {
        http_worker(base_url, req_rx);
    });

    // Main loop
    loop {
        if !app.running {
            break;
        }

        // Non-blocking check for pending request and status updates
        // Use take pattern to avoid nested borrows
        if app.pending_request.is_some() {
            let mut statuses = Vec::new();
            let mut result_val = None;

            // Re-borrow to check/drain
            if let Some(ref mut pending) = app.pending_request {
                // Drain status updates
                while let Ok(status) = pending.status_rx.try_recv() {
                    statuses.push(status);
                }
                // Try to get result (don't remove yet)
                if let Ok(r) = pending.response_rx.try_recv() {
                    result_val = Some(r);
                }
            }

            // Add status messages to conversation (now safe, no pending borrow)
            for status in statuses {
                app.add_system(status);
            }

            // If response ready, process it (need to re-borrow once more)
            if result_val.is_some() {
                if let Some(ref mut pending) = app.pending_request {
                    if let Ok(result) = pending.response_rx.try_recv() {
                        app.waiting = false;
                        match result {
                            Ok(run_resp) => {
                                app.session_id = Some(run_resp.session_id.clone());
                                app.add_assistant(
                                    run_resp.result,
                                    Some(format!(
                                        "{} iter, {} tools",
                                        run_resp.iterations, run_resp.tool_calls
                                    )),
                                );
                            }
                            Err(e) => {
                                app.add_error(format!("Error: {}", e));
                            }
                        }
                        app.pending_request = None;
                        app.waiting_dots = 0;
                    }
                }
            }
        }

        // Animate waiting state
        if app.waiting {
            app.waiting_dots = (app.waiting_dots + 1) % 8;
        }

        draw(&mut terminal, &app).ok();

        if crossterm::event::poll(std::time::Duration::from_millis(100)).is_err() {
            continue;
        }

        if let Ok(crossterm::event::Event::Key(key)) = crossterm::event::read() {
            if key.code == crossterm::event::KeyCode::Char('c')
                && key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL)
            {
                break;
            }
            handle_key(&mut app, key, &req_tx);
        }
    }

    // Cleanup
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen
    )?;
    crossterm::terminal::disable_raw_mode()?;
    println!("Goodbye!");

    Ok(())
}
