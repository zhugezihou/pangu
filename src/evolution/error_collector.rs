//! Error Collector - 收集并记录执行错误

use std::sync::{Arc, RwLock};
use crate::core::ErrorRecord;
use crate::memory::episodic::EpisodicMemory;

#[derive(Debug, Clone)]
pub struct ErrorCollector {
    errors: Arc<RwLock<Vec<ErrorRecord>>>,
    episodic: Option<Arc<EpisodicMemory>>,
    error_threshold: u32,
}

impl ErrorCollector {
    pub fn new(episodic: Option<Arc<EpisodicMemory>>) -> Self {
        Self {
            errors: Arc::new(RwLock::new(Vec::new())),
            episodic,
            error_threshold: 3,
        }
    }

    pub fn with_threshold(mut self, threshold: u32) -> Self {
        self.error_threshold = threshold;
        self
    }

    pub fn record(
        &self,
        session_id: &str,
        task: &str,
        error_type: &str,
        error_message: &str,
        context: &str,
    ) {
        let record = ErrorRecord::new(session_id, task, error_type, error_message, context);
        if let Some(ep) = &self.episodic {
            let _ = ep.store_error(&record);
        }
        self.errors.write().unwrap().push(record);
    }

    pub fn update_attempt(&self, error_type: &str) {
        let mut errors = self.errors.write().unwrap();
        if let Some(last) = errors.last_mut() {
            if last.error_type == error_type {
                last.increment_attempt();
            }
        }
    }

    pub fn mark_resolved(&self, error_type: &str) {
        let mut errors = self.errors.write().unwrap();
        if let Some(last) = errors.last_mut() {
            if last.error_type == error_type {
                last.resolved = true;
            }
        }
    }

    pub fn should_evolve(&self, error_type: &str) -> bool {
        let errors = self.errors.read().unwrap();
        errors.iter().rev()
            .find(|e| e.error_type == error_type)
            .map(|e| e.attempts >= self.error_threshold && !e.resolved)
            .unwrap_or(false)
    }

    pub fn recent_errors(&self, count: usize) -> Vec<ErrorRecord> {
        let errors = self.errors.read().unwrap();
        errors.iter().rev().take(count).cloned().collect()
    }
}
