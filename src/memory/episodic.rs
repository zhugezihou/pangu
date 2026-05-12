//! Episodic Memory - 情景记忆（SQLite），完整事件记录

use rusqlite::{Connection, params};
use std::path::PathBuf;
use crate::core::{ErrorRecord, MemoryEntry, MemoryType};
use chrono::{DateTime, Utc};

#[derive(Debug)]
pub struct EpisodicMemory {
    conn: Connection,
}

impl EpisodicMemory {
    pub fn new(db_path: &PathBuf) -> anyhow::Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS episodes (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                tool_name TEXT,
                success INTEGER,
                created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS errors (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                task TEXT NOT NULL,
                error_type TEXT NOT NULL,
                error_message TEXT NOT NULL,
                context TEXT NOT NULL,
                attempts INTEGER DEFAULT 1,
                resolved INTEGER DEFAULT 0,
                created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS skills (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                description TEXT NOT NULL,
                source_code TEXT NOT NULL,
                tool_name TEXT NOT NULL,
                parameters TEXT NOT NULL,
                success_count INTEGER DEFAULT 0,
                failure_count INTEGER DEFAULT 0,
                created_at TEXT NOT NULL,
                last_used TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_episodes_session ON episodes(session_id);
            CREATE INDEX IF NOT EXISTS idx_errors_session ON errors(session_id);
            "
        )?;
        Ok(Self { conn })
    }

    pub fn store_episode(
        &self,
        session_id: &str,
        role: &str,
        content: &str,
        tool_name: Option<&str>,
        success: Option<bool>,
    ) -> anyhow::Result<()> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO episodes (id, session_id, role, content, tool_name, success, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, session_id, role, content, tool_name, success.map(|s| s as i32), now],
        )?;
        Ok(())
    }

    pub fn store_error(&self, err: &ErrorRecord) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO errors (id, session_id, task, error_type, error_message, context, attempts, resolved, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                err.id, err.session_id, err.task, err.error_type,
                err.error_message, err.context, err.attempts,
                err.resolved as i32, err.created_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn get_session_episodes(&self, session_id: &str, limit: usize) -> anyhow::Result<Vec<serde_json::Value>> {
        let mut stmt = self.conn.prepare(
            "SELECT role, content, tool_name, success, created_at FROM episodes
             WHERE session_id = ?1 ORDER BY created_at DESC LIMIT ?2"
        )?;
        let rows = stmt.query_map(params![session_id, limit], |row| {
            Ok(serde_json::json!({
                "role": row.get::<_, String>(0)?,
                "content": row.get::<_, String>(1)?,
                "tool_name": row.get::<_, Option<String>>(2)?,
                "success": row.get::<_, Option<i32>>(3)?,
                "created_at": row.get::<_, String>(4)?,
            }))
        })?;
        let mut result = vec![];
        for r in rows {
            result.push(r?);
        }
        Ok(result)
    }

    pub fn store_skill(&self, skill: &crate::core::Skill) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO skills (id, name, description, source_code, tool_name, parameters, success_count, failure_count, created_at, last_used)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                skill.id, skill.name, skill.description, skill.source_code,
                skill.tool_name, serde_json::to_string(&skill.parameters)?,
                skill.success_count, skill.failure_count,
                skill.created_at.to_rfc3339(),
                skill.last_used.map(|d| d.to_rfc3339()),
            ],
        )?;
        Ok(())
    }

    pub fn get_all_skills(&self) -> anyhow::Result<Vec<crate::core::Skill>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, description, source_code, tool_name, parameters, success_count, failure_count, created_at, last_used FROM skills"
        )?;
        let rows = stmt.query_map([], |row| {
            let params_str: String = row.get(5)?;
            let params: serde_json::Value = serde_json::from_str(&params_str).unwrap_or(serde_json::Value::Null);
            let last_used: Option<String> = row.get(9)?;
            Ok(crate::core::Skill {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                source_code: row.get(3)?,
                tool_name: row.get(4)?,
                parameters: params,
                success_count: row.get::<_, i32>(6)? as u32,
                failure_count: row.get::<_, i32>(7)? as u32,
                created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(8)?)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                last_used: last_used.and_then(|s| DateTime::parse_from_rfc3339(&s).ok().map(|d| d.with_timezone(&Utc))),
            })
        })?;
        let mut skills = vec![];
        for r in rows {
            skills.push(r?);
        }
        Ok(skills)
    }
}
