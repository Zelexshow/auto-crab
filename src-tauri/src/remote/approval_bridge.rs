use crate::config::AppConfig;
use crate::remote::feishu::FeishuBot;
use crate::remote::wechat_work::WechatWorkBot;
use crate::security::approval::PendingApproval;
use anyhow::Result;

/// Bridges the local approval system with remote channels.
/// When a dangerous operation needs approval locally, this module
/// can forward the approval request to Feishu/WeChat Work so the user
/// can approve from their phone.
pub struct RemoteApprovalBridge {
    feishu: Option<FeishuBot>,
    wechat: Option<WechatWorkBot>,
}

impl RemoteApprovalBridge {
    pub fn from_config(config: &AppConfig) -> Self {
        let feishu = config
            .remote
            .feishu
            .as_ref()
            .map(|c| FeishuBot::new(c.clone()));
        let wechat = config
            .remote
            .wechat_work
            .as_ref()
            .map(|c| WechatWorkBot::new(c.clone()));
        Self { feishu, wechat }
    }

    pub fn is_enabled(&self) -> bool {
        self.feishu.is_some() || self.wechat.is_some()
    }

    /// Push an approval request to all configured remote channels.
    /// Returns the list of channels that were notified.
    pub async fn push_approval(&mut self, approval: &PendingApproval) -> Result<Vec<String>> {
        let mut notified = Vec::new();
        let msg = format_approval_message(approval);

        if let Some(ref mut feishu) = self.feishu {
            match feishu.send_message("", &msg).await {
                Ok(()) => {
                    notified.push("feishu".into());
                    tracing::info!("Approval pushed to Feishu: {}", approval.id);
                }
                Err(e) => {
                    tracing::warn!("Failed to push approval to Feishu: {}", e);
                }
            }
        }

        if let Some(ref mut wechat) = self.wechat {
            match wechat.send_message("", &msg).await {
                Ok(()) => {
                    notified.push("wechat_work".into());
                    tracing::info!("Approval pushed to WeChat Work: {}", approval.id);
                }
                Err(e) => {
                    tracing::warn!("Failed to push approval to WeChat Work: {}", e);
                }
            }
        }

        Ok(notified)
    }

    /// Push an approval result notification back to remote channels.
    pub async fn notify_result(&mut self, approval_id: &str, approved: bool, operation: &str) {
        let emoji = if approved { "✅" } else { "❌" };
        let status = if approved { "已批准" } else { "已拒绝" };
        let msg = format!(
            "{} 操作{}: {}\nID: {}",
            emoji, status, operation, approval_id
        );

        if let Some(ref mut feishu) = self.feishu {
            let _ = feishu.send_message("", &msg).await;
        }
        if let Some(ref mut wechat) = self.wechat {
            let _ = wechat.send_message("", &msg).await;
        }
    }
}

fn format_approval_message(approval: &PendingApproval) -> String {
    let risk = match approval.risk_level {
        crate::config::RiskLevel::Safe => "🟢 安全",
        crate::config::RiskLevel::Moderate => "🟡 中风险",
        crate::config::RiskLevel::Dangerous => "🔴 高风险",
        crate::config::RiskLevel::Forbidden => "⛔ 禁止",
    };

    format!(
        "🦀 Auto Crab 操作审批\n\n\
         操作: {}\n\
         风险: {}\n\
         说明: {}\n\
         ID: {}\n\n\
         回复 /approve {} 批准\n\
         回复 /reject {} 拒绝",
        approval.operation, risk, approval.description, approval.id, approval.id, approval.id,
    )
}
