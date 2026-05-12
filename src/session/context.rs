//! Session Context - 会话管理

use std::path::PathBuf;
use crate::core::SessionId;
use crate::memory::{WorkingMemory, EpisodicMemory, SemanticMemory};

pub struct SessionContext {
    pub session_id: SessionId,
    pub working_memory: WorkingMemory,
    pub episodic_memory: Option<EpisodicMemory>,
    pub semantic_memory: Option<SemanticMemory>,
    pub data_dir: PathBuf,
}

impl SessionContext {
    pub fn new(session_id: SessionId, working_window: usize, data_dir: PathBuf) -> anyhow::Result<Self> {
        let working_memory = WorkingMemory::new(working_window);
        let episodic_path = data_dir.join("episodic.db");
        let semantic_path = data_dir.join("semantic.db");
        
        let episodic_memory = EpisodicMemory::new(&episodic_path).ok();
        let semantic_memory = SemanticMemory::new(&semantic_path, 384).ok();

        Ok(Self {
            session_id,
            working_memory,
            episodic_memory,
            semantic_memory,
            data_dir,
        })
    }

    pub fn with_episodic(mut self, episodic: EpisodicMemory) -> Self {
        self.episodic_memory = Some(episodic);
        self
    }
}
