#![allow(dead_code)]

//! Cognitive observability — structured JSONL trace writers.
//!
//! Three trace files (all append-only JSONL in APP_LOG_DIR):
//!   cognitive_timeline.jsonl — routing + execution decisions
//!   planner_trace.jsonl      — planner decisions and graph structure
//!   memory_trace.jsonl       — memory retrieval hits/misses

use std::io::Write;
use std::path::PathBuf;
use std::time::SystemTime;
use jarvis_core::APP_LOG_DIR;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
}

fn append_line(path: &PathBuf, line: &str) {
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true).append(true)
        .open(path)
    {
        let _ = writeln!(f, "{}", line);
    }
}

// ── Trace ─────────────────────────────────────────────────────────────────────

pub struct CognitiveTrace {
    timeline_path: PathBuf,
    planner_path: PathBuf,
    memory_path: PathBuf,
    enabled: bool,
}

impl CognitiveTrace {
    pub fn new() -> Self {
        let (timeline_path, planner_path, memory_path, enabled) =
            if let Some(dir) = APP_LOG_DIR.get() {
                (
                    dir.join("cognitive_timeline.jsonl"),
                    dir.join("planner_trace.jsonl"),
                    dir.join("memory_trace.jsonl"),
                    true,
                )
            } else {
                (PathBuf::new(), PathBuf::new(), PathBuf::new(), false)
            };
        Self { timeline_path, planner_path, memory_path, enabled }
    }

    // ── Routing ───────────────────────────────────────────────────────────────

    /// Log a routing decision.
    /// `decision`: "execute" | "clarify" | "plan" | "reject"
    pub fn log_routing(
        &self,
        text: &str,
        domain: &str,
        decision: &str,
        confidence: f64,
        latency_ms: u64,
    ) {
        if !self.enabled { return; }
        let line = format!(
            "{{\"ts\":{},\"event\":\"routing\",\"text\":\"{}\",\"domain\":\"{}\",\
             \"decision\":\"{}\",\"confidence\":{:.3},\"latency_ms\":{}}}",
            now_ms(), esc(text), domain, decision, confidence, latency_ms
        );
        append_line(&self.timeline_path, &line);
    }

    /// Log an execution result.
    pub fn log_execution(
        &self,
        command: &str,
        source: &str,
        success: bool,
        latency_ms: u64,
    ) {
        if !self.enabled { return; }
        let line = format!(
            "{{\"ts\":{},\"event\":\"execution\",\"command\":\"{}\",\"source\":\"{}\",\
             \"success\":{},\"latency_ms\":{}}}",
            now_ms(), esc(command), source, success, latency_ms
        );
        append_line(&self.timeline_path, &line);
    }

    /// Log a clarification request.
    pub fn log_clarification(&self, text: &str, question: &str, domain: &str) {
        if !self.enabled { return; }
        let line = format!(
            "{{\"ts\":{},\"event\":\"clarification\",\"text\":\"{}\",\"question\":\"{}\",\"domain\":\"{}\"}}",
            now_ms(), esc(text), esc(question), domain
        );
        append_line(&self.timeline_path, &line);
    }

    /// Log a hallucination block.
    pub fn log_containment_block(&self, source: &str, reason: &str) {
        if !self.enabled { return; }
        let line = format!(
            "{{\"ts\":{},\"event\":\"hallucination_blocked\",\"source\":\"{}\",\"reason\":\"{}\"}}",
            now_ms(), source, esc(reason)
        );
        append_line(&self.timeline_path, &line);
    }

    // ── Planner ───────────────────────────────────────────────────────────────

    /// Log a plan generation.
    pub fn log_plan_generated(
        &self,
        goal: &str,
        step_count: usize,
        origin: &str,
        latency_ms: u64,
    ) {
        if !self.enabled { return; }
        let line = format!(
            "{{\"ts\":{},\"event\":\"plan_generated\",\"goal\":\"{}\",\"steps\":{},\
             \"origin\":\"{}\",\"latency_ms\":{}}}",
            now_ms(), esc(goal), step_count, origin, latency_ms
        );
        append_line(&self.planner_path, &line);
    }

    /// Log a plan validation failure.
    pub fn log_plan_rejected(&self, goal: &str, reason: &str) {
        if !self.enabled { return; }
        let line = format!(
            "{{\"ts\":{},\"event\":\"plan_rejected\",\"goal\":\"{}\",\"reason\":\"{}\"}}",
            now_ms(), esc(goal), esc(reason)
        );
        append_line(&self.planner_path, &line);
    }

    /// Log a plan execution result.
    pub fn log_plan_executed(
        &self,
        goal: &str,
        total_steps: usize,
        succeeded: usize,
        aborted: bool,
    ) {
        if !self.enabled { return; }
        let line = format!(
            "{{\"ts\":{},\"event\":\"plan_executed\",\"goal\":\"{}\",\"total\":{},\
             \"succeeded\":{},\"aborted\":{}}}",
            now_ms(), esc(goal), total_steps, succeeded, aborted
        );
        append_line(&self.planner_path, &line);
    }

    // ── Memory ────────────────────────────────────────────────────────────────

    /// Log a memory retrieval.
    pub fn log_memory_retrieval(
        &self,
        kind: &str,          // "working" | "episodic" | "semantic" | "task"
        query: &str,
        hits: usize,
        latency_ms: u64,
    ) {
        if !self.enabled { return; }
        let line = format!(
            "{{\"ts\":{},\"event\":\"memory_retrieval\",\"kind\":\"{}\",\"query\":\"{}\",\
             \"hits\":{},\"latency_ms\":{}}}",
            now_ms(), kind, esc(query), hits, latency_ms
        );
        append_line(&self.memory_path, &line);
    }

    /// Log a memory write.
    pub fn log_memory_write(&self, kind: &str, description: &str) {
        if !self.enabled { return; }
        let line = format!(
            "{{\"ts\":{},\"event\":\"memory_write\",\"kind\":\"{}\",\"description\":\"{}\"}}",
            now_ms(), kind, esc(description)
        );
        append_line(&self.memory_path, &line);
    }

    /// Log a memory expiry.
    pub fn log_memory_expired(&self, kind: &str, count: usize) {
        if !self.enabled { return; }
        let line = format!(
            "{{\"ts\":{},\"event\":\"memory_expired\",\"kind\":\"{}\",\"count\":{}}}",
            now_ms(), kind, count
        );
        append_line(&self.memory_path, &line);
    }
}
