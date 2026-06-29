use hybrid_core::{BuildTask, ChangeEvent, TaskKind};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;

#[derive(Debug, Error)]
pub enum SchedulerError {
    #[error("queue is empty")]
    Empty,
}

#[derive(Debug, Default, Clone)]
pub struct CancellationToken(Arc<AtomicU64>);

impl CancellationToken {
    pub fn current(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }

    pub fn cancel_prior_and_next(&self) -> u64 {
        self.0.fetch_add(1, Ordering::SeqCst) + 1
    }
}

#[derive(Clone, Default)]
pub struct BuildScheduler {
    queue: Arc<Mutex<VecDeque<BuildTask>>>,
    token: CancellationToken,
}

impl BuildScheduler {
    pub async fn enqueue_wave(&self, event: &ChangeEvent) {
        let generation = self.token.cancel_prior_and_next();
        let build_id = format!("{}-{generation}", event.module_id);
        let mut q = self.queue.lock().await;
        q.clear();
        q.push_back(BuildTask {
            task_kind: TaskKind::AstRefresh,
            build_id: build_id.clone(),
            deps: vec![],
            cancel_token: generation.to_string(),
        });
        q.push_back(BuildTask {
            task_kind: TaskKind::SwiftFrontend,
            build_id: build_id.clone(),
            deps: vec!["AstRefresh".to_string()],
            cancel_token: generation.to_string(),
        });
        q.push_back(BuildTask {
            task_kind: TaskKind::SilAnalysis,
            build_id,
            deps: vec!["SwiftFrontend".to_string()],
            cancel_token: generation.to_string(),
        });
    }

    pub async fn next_task(&self) -> Result<BuildTask, SchedulerError> {
        self.queue
            .lock()
            .await
            .pop_front()
            .ok_or(SchedulerError::Empty)
    }

    pub fn is_cancelled(&self, token: &str) -> bool {
        match token.parse::<u64>() {
            Ok(t) => t != self.token.current(),
            Err(_) => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn newest_wave_cancels_old_wave() {
        let scheduler = BuildScheduler::default();
        scheduler
            .enqueue_wave(&ChangeEvent {
                path: "A.swift".to_string(),
                module_id: "App".to_string(),
                hash: "1".to_string(),
                timestamp_ms: 1,
            })
            .await;
        let old = scheduler.next_task().await.expect("old first");
        scheduler
            .enqueue_wave(&ChangeEvent {
                path: "B.swift".to_string(),
                module_id: "App".to_string(),
                hash: "2".to_string(),
                timestamp_ms: 2,
            })
            .await;
        assert!(scheduler.is_cancelled(&old.cancel_token));
    }

    #[tokio::test]
    async fn next_task_empty_queue_returns_error() {
        let scheduler = BuildScheduler::default();
        let result = scheduler.next_task().await;
        assert!(matches!(result, Err(SchedulerError::Empty)));
    }
}
