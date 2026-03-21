use crate::config::ScheduledJob;
use chrono::{Local, NaiveTime};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledTaskStatus {
    pub name: String,
    pub cron: String,
    pub last_run: Option<String>,
    pub next_run: Option<String>,
    pub enabled: bool,
    pub auto_execute: bool,
}

pub struct TaskScheduler {
    jobs: Vec<ScheduledJob>,
    require_confirmation: bool,
    last_runs: HashMap<String, chrono::DateTime<Local>>,
    event_tx: Option<mpsc::Sender<SchedulerEvent>>,
}

#[derive(Debug, Clone)]
pub enum SchedulerEvent {
    TaskReady { name: String, action: String, auto_execute: bool },
    TaskCompleted { name: String, success: bool },
}

impl TaskScheduler {
    pub fn new(jobs: Vec<ScheduledJob>, require_confirmation: bool) -> Self {
        Self {
            jobs,
            require_confirmation,
            last_runs: HashMap::new(),
            event_tx: None,
        }
    }

    pub fn set_event_sender(&mut self, tx: mpsc::Sender<SchedulerEvent>) {
        self.event_tx = Some(tx);
    }

    /// Check which jobs are due. Simple interval-based check.
    /// For a production cron parser, use cron crate. Here we do basic HH:MM matching.
    pub fn check_due_jobs(&mut self) -> Vec<ScheduledJob> {
        let now = Local::now();
        let current_time = now.format("%H:%M").to_string();
        let mut due = Vec::new();

        for job in &self.jobs {
            if let Some(time_part) = parse_simple_cron(&job.cron) {
                if time_part == current_time {
                    let should_run = match self.last_runs.get(&job.name) {
                        Some(last) => now.signed_duration_since(*last).num_minutes() > 1,
                        None => true,
                    };

                    if should_run {
                        self.last_runs.insert(job.name.clone(), now);
                        due.push(job.clone());

                        if let Some(ref tx) = self.event_tx {
                            let _ = tx.try_send(SchedulerEvent::TaskReady {
                                name: job.name.clone(),
                                action: job.action.clone(),
                                auto_execute: job.auto_execute && !self.require_confirmation,
                            });
                        }
                    }
                }
            }
        }

        due
    }

    pub fn list_status(&self) -> Vec<ScheduledTaskStatus> {
        self.jobs.iter().map(|job| {
            ScheduledTaskStatus {
                name: job.name.clone(),
                cron: job.cron.clone(),
                last_run: self.last_runs.get(&job.name).map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string()),
                next_run: None,
                enabled: true,
                auto_execute: job.auto_execute,
            }
        }).collect()
    }
}

/// Parse a simple cron expression. Supports:
/// - "0 9 * * *" -> extracts "09:00"
/// - "30 14 * * *" -> extracts "14:30"
fn parse_simple_cron(cron: &str) -> Option<String> {
    let parts: Vec<&str> = cron.split_whitespace().collect();
    if parts.len() >= 2 {
        let minute: u32 = parts[0].parse().ok()?;
        let hour: u32 = parts[1].parse().ok()?;
        let _ = NaiveTime::from_hms_opt(hour, minute, 0)?;
        Some(format!("{:02}:{:02}", hour, minute))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cron() {
        assert_eq!(parse_simple_cron("0 9 * * *"), Some("09:00".into()));
        assert_eq!(parse_simple_cron("30 14 * * *"), Some("14:30".into()));
        assert_eq!(parse_simple_cron("invalid"), None);
    }
}
