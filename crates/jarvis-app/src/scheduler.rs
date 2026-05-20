#![allow(dead_code)]

use std::sync::Arc;
use std::time::SystemTime;
use crate::bus::{BusEvent, CognitiveBus};

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Job types ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum JobType {
    WorkflowTrigger { workflow_id: String },
    HealthCheck,
    MemoryCleanup,
    CognitiveReport,
    Custom { tag: String },
}

impl JobType {
    pub fn as_str(&self) -> &str {
        match self {
            JobType::WorkflowTrigger { .. } => "workflow_trigger",
            JobType::HealthCheck => "health_check",
            JobType::MemoryCleanup => "memory_cleanup",
            JobType::CognitiveReport => "cognitive_report",
            JobType::Custom { tag } => tag.as_str(),
        }
    }
}

// ── Scheduled job ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ScheduledJob {
    pub id: String,
    pub job_type: JobType,
    pub due_at_ms: u64,
    /// If Some(ms), re-schedule after firing.
    pub repeat_interval_ms: Option<u64>,
}

impl ScheduledJob {
    pub fn once(id: impl Into<String>, job_type: JobType, due_at_ms: u64) -> Self {
        Self { id: id.into(), job_type, due_at_ms, repeat_interval_ms: None }
    }

    pub fn recurring(id: impl Into<String>, job_type: JobType, first_due_ms: u64, interval_ms: u64) -> Self {
        Self { id: id.into(), job_type, due_at_ms: first_due_ms, repeat_interval_ms: Some(interval_ms) }
    }

    pub fn in_ms(id: impl Into<String>, job_type: JobType, delay_ms: u64) -> Self {
        Self::once(id, job_type, now_ms() + delay_ms)
    }
}

// ── Cognitive scheduler ───────────────────────────────────────────────────────

pub struct CognitiveScheduler {
    jobs: Vec<ScheduledJob>,
    bus: Arc<CognitiveBus>,
}

impl CognitiveScheduler {
    pub fn new(bus: Arc<CognitiveBus>) -> Self {
        Self { jobs: Vec::new(), bus }
    }

    pub fn schedule(&mut self, job: ScheduledJob) {
        debug!("[SCHEDULER] Scheduled '{}' type={} due_ms={}", job.id, job.job_type.as_str(), job.due_at_ms);
        self.bus.publish(BusEvent::JobScheduled {
            id: job.id.clone(),
            job_type: job.job_type.as_str().to_string(),
            due_at_ms: job.due_at_ms,
        });
        self.jobs.push(job);
    }

    /// Call periodically from the main loop. Returns fired jobs for the caller to act on.
    pub fn tick(&mut self) -> Vec<ScheduledJob> {
        let now = now_ms();
        let mut fired = Vec::new();
        let mut reschedule = Vec::new();
        let mut remaining = Vec::new();

        for job in self.jobs.drain(..) {
            if job.due_at_ms <= now {
                if let Some(interval) = job.repeat_interval_ms {
                    reschedule.push(ScheduledJob {
                        id: job.id.clone(),
                        job_type: job.job_type.clone(),
                        due_at_ms: now + interval,
                        repeat_interval_ms: Some(interval),
                    });
                }
                fired.push(job);
            } else {
                remaining.push(job);
            }
        }

        self.jobs = remaining;
        self.jobs.extend(reschedule);

        for job in &fired {
            self.bus.publish(BusEvent::JobExecuted { id: job.id.clone(), success: true });
        }

        fired
    }

    pub fn cancel(&mut self, id: &str) {
        self.jobs.retain(|j| j.id != id);
    }

    pub fn pending_count(&self) -> usize {
        self.jobs.len()
    }
}
