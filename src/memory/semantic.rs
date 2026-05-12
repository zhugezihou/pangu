//! Semantic Memory - 语义记忆，简单向量存储（用点积相似度）

use rusqlite::{Connection, params};
use std::path::PathBuf;

pub struct SemanticMemory {
    conn: Connection,
    dim: usize,
}

impl SemanticMemory {
    pub fn new(db_path: &PathBuf, dim: usize) -> anyhow::Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(db_path)?;
        conn.execute_batch(&format!(
            "CREATE TABLE IF NOT EXISTS embeddings (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                vector BLOB NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_embeddings_id ON embeddings(id);"
        ))?;
        Ok(Self { conn, dim })
    }

    /// 简单 cosine 相似度（用点积近似）
    fn cosine(vec_a: &[f32], vec_b: &[f32]) -> f32 {
        let dot: f32 = vec_a.iter().zip(vec_b.iter()).map(|(a, b)| a * b).sum();
        let norm_a: f32 = vec_a.iter().map(|v| v * v).sum::<f32>().sqrt();
        let norm_b: f32 = vec_b.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm_a == 0.0 || norm_b == 0.0 { 0.0 } else { dot / (norm_a * norm_b) }
    }

    pub fn store(&self, id: &str, content: &str, vector: &[f32]) -> anyhow::Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        let vec_bytes: Vec<u8> = vector.iter().flat_map(|f| f.to_le_bytes()).collect();
        self.conn.execute(
            "INSERT OR REPLACE INTO embeddings (id, content, vector, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![id, content, vec_bytes, now],
        )?;
        Ok(())
    }

    pub fn search(&self, query: &[f32], top_k: usize) -> anyhow::Result<Vec<(String, String, f32)>> {
        let mut stmt = self.conn.prepare("SELECT id, content, vector FROM embeddings")?;
        let rows = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let content: String = row.get(1)?;
            let vec_bytes: Vec<u8> = row.get(2)?;
            let vector: Vec<f32> = vec_bytes.chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect();
            Ok((id, content, vector))
        })?;
        let mut results: Vec<_> = rows.filter_map(|r| r.ok())
            .map(|(id, content, vector)| (id, content, Self::cosine(query, &vector)))
            .collect();
        results.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap());
        Ok(results.into_iter().take(top_k).map(|(id, content, score)| (id, content, score)).collect())
    }
}
