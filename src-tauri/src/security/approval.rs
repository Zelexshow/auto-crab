use crate::config::RiskLevel;
use crate::security::risk::RiskEngine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingApproval {
    pub id: String,
    pub operation: String,
    pub risk_level: RiskLevel,
    pub description: String,
    pub details: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApprovalDecision {
    Approved,
    Rejected { reason: String },
}

type ApprovalSender = oneshot::Sender<ApprovalDecision>;

pub struct ApprovalGate {
    risk_engine: RiskEngine,
    pending: Arc<Mutex<HashMap<String, (PendingApproval, ApprovalSender)>>>,
}

impl ApprovalGate {
    pub fn new(risk_engine: RiskEngine) -> Self {
        Self {
            risk_engine,
            pending: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Request approval for an operation.
    /// Returns immediately for Safe operations.
    /// Returns a PendingApproval for Moderate/Dangerous operations.
    /// Returns Err for Forbidden operations.
    pub async fn request(
        &self,
        operation: &str,
        description: &str,
        details: serde_json::Value,
    ) -> anyhow::Result<ApprovalResult> {
        let risk = self.risk_engine.assess(operation);

        match risk {
            RiskLevel::Safe => Ok(ApprovalResult::AutoApproved),
            RiskLevel::Forbidden => {
                anyhow::bail!(
                    "Operation '{}' is forbidden and cannot be executed",
                    operation
                );
            }
            RiskLevel::Moderate | RiskLevel::Dangerous => {
                let (tx, rx) = oneshot::channel();
                let approval = PendingApproval {
                    id: Uuid::new_v4().to_string(),
                    operation: operation.to_string(),
                    risk_level: risk,
                    description: description.to_string(),
                    details,
                    created_at: chrono::Utc::now(),
                };
                let id = approval.id.clone();

                {
                    let mut pending = self.pending.lock().await;
                    pending.insert(id.clone(), (approval.clone(), tx));
                }

                Ok(ApprovalResult::Pending {
                    approval,
                    receiver: rx,
                })
            }
        }
    }

    pub async fn decide(&self, id: &str, decision: ApprovalDecision) -> anyhow::Result<()> {
        let mut pending = self.pending.lock().await;
        if let Some((_, sender)) = pending.remove(id) {
            let _ = sender.send(decision);
            Ok(())
        } else {
            anyhow::bail!("No pending approval with id '{}'", id);
        }
    }

    pub async fn list_pending(&self) -> Vec<PendingApproval> {
        let pending = self.pending.lock().await;
        pending.values().map(|(a, _)| a.clone()).collect()
    }
}

pub enum ApprovalResult {
    AutoApproved,
    Pending {
        approval: PendingApproval,
        receiver: oneshot::Receiver<ApprovalDecision>,
    },
}
