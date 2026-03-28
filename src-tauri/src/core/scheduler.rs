use crate::config::ScheduledJob;
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
}

#[derive(Debug, Clone)]
pub struct DueJob {
    pub name: String,
    pub action: String,
    pub auto_execute: bool,
    pub skill_ref: Option<String>,
}

impl TaskScheduler {
    pub fn new(jobs: Vec<ScheduledJob>, require_confirmation: bool) -> Self {
        Self {
            jobs,
            require_confirmation,
            last_runs: HashMap::new(),
        }
    }

    pub fn check_due_jobs(&mut self) -> Vec<DueJob> {
        let now = Local::now();
        let mut due = Vec::new();

        for job in &self.jobs {
            if is_cron_match(&job.cron, &now) {
                let should_run = match self.last_runs.get(&job.name) {
                    Some(last) => now.signed_duration_since(*last).num_minutes() > 1,
                    None => true,
                };

                if should_run {
                    self.last_runs.insert(job.name.clone(), now);
                    due.push(DueJob {
                        name: job.name.clone(),
                        action: job.action.clone(),
                        auto_execute: job.auto_execute && !self.require_confirmation,
                        skill_ref: job.skill_ref.clone(),
                    });
                }
            }
        }

        due
    }

    pub fn list_status(&self) -> Vec<ScheduledTaskStatus> {
        self.jobs.iter().map(|job| ScheduledTaskStatus {
            name: job.name.clone(),
            cron: job.cron.clone(),
            last_run: self.last_runs.get(&job.name)
                .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string()),
            next_run: next_run_str(&job.cron),
            enabled: true,
            auto_execute: job.auto_execute,
        }).collect()
    }
}

fn is_cron_match(cron_expr: &str, now: &chrono::DateTime<Local>) -> bool {
    let parts: Vec<&str> = cron_expr.split_whitespace().collect();
    if parts.len() < 5 { return false; }

    let now_min = now.format("%M").to_string().parse::<u32>().unwrap_or(99);
    let now_hour = now.format("%H").to_string().parse::<u32>().unwrap_or(99);
    let now_dom = now.format("%d").to_string().parse::<u32>().unwrap_or(99);
    let now_dow = now.format("%u").to_string().parse::<u32>().unwrap_or(99); // 1=Mon, 7=Sun
    let now_dow_cron = if now_dow == 7 { 0 } else { now_dow }; // cron: 0=Sun

    let matches_field = |field: &str, value: u32| -> bool {
        if field == "*" { return true; }
        for part in field.split(',') {
            if part.contains('-') {
                let range: Vec<&str> = part.split('-').collect();
                if let (Ok(from), Ok(to)) = (range[0].parse::<u32>(), range.get(1).unwrap_or(&"0").parse::<u32>()) {
                    if value >= from && value <= to { return true; }
                }
            } else if let Ok(v) = part.parse::<u32>() {
                if v == value { return true; }
            }
        }
        false
    };

    matches_field(parts[0], now_min)
        && matches_field(parts[1], now_hour)
        && matches_field(parts[2], now_dom)
        && matches_field(parts[4], now_dow_cron)
}

fn next_run_str(cron_expr: &str) -> Option<String> {
    let cron_7 = format!("0 {} *", cron_expr);
    cron::Schedule::from_str(&cron_7).ok().and_then(|s| {
        s.upcoming(chrono::Utc).next().map(|t| {
            t.with_timezone(&Local).format("%m-%d %H:%M").to_string()
        })
    })
}

use std::str::FromStr;
