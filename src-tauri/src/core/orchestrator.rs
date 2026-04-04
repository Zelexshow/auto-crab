use crate::models::provider::*;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct SubAgentTask {
    pub id: String,
    pub role: String,
    pub prompt: String,
    pub tools_enabled: bool,
    pub max_rounds: usize,
}

#[derive(Debug, Clone)]
pub struct SubAgentResult {
    pub task_id: String,
    pub role: String,
    pub output: String,
    pub success: bool,
}

pub struct Orchestrator {
    provider: Arc<dyn ModelProvider>,
    cfg: crate::config::AppConfig,
}

impl Orchestrator {
    pub fn new(provider: Arc<dyn ModelProvider>, cfg: crate::config::AppConfig) -> Self {
        Self { provider, cfg }
    }

    /// Launch N sub-agents in parallel and collect their results.
    pub async fn fan_out(
        &self,
        base_system_prompt: &str,
        tasks: Vec<SubAgentTask>,
        sink: &dyn super::engine::EventSink,
        stream_id: &str,
    ) -> Vec<SubAgentResult> {
        use tokio::task::JoinSet;

        let mut join_set = JoinSet::new();

        for task in tasks {
            let provider = self.provider.clone();
            let system_prompt = base_system_prompt.to_string();
            let cfg = self.cfg.clone();

            join_set.spawn(async move {
                let result = run_sub_agent(&provider, &cfg, &system_prompt, &task).await;
                SubAgentResult {
                    task_id: task.id.clone(),
                    role: task.role.clone(),
                    output: result,
                    success: true,
                }
            });
        }

        let mut results = Vec::new();
        while let Some(res) = join_set.join_next().await {
            match res {
                Ok(sub_result) => {
                    tracing::info!(
                        "[Orchestrator] Sub-agent '{}' ({}) completed, output_len={}",
                        sub_result.task_id, sub_result.role, sub_result.output.len()
                    );
                    let _ = sink; // sink is available for progress reporting
                    let _ = stream_id;
                    results.push(sub_result);
                }
                Err(e) => {
                    tracing::error!("[Orchestrator] Sub-agent join error: {}", e);
                    results.push(SubAgentResult {
                        task_id: "error".into(),
                        role: "error".into(),
                        output: format!("Sub-agent failed: {}", e),
                        success: false,
                    });
                }
            }
        }

        results
    }

    /// Merge multiple sub-agent results into a consolidated context string.
    pub fn fan_in(&self, results: &[SubAgentResult]) -> String {
        let mut parts = Vec::new();
        for (i, r) in results.iter().enumerate() {
            let status = if r.success { "completed" } else { "failed" };
            let output_preview: String = r.output.chars().take(2000).collect();
            parts.push(format!(
                "--- Sub-agent {} [{}] ({}) ---\n{}",
                i + 1, r.role, status, output_preview
            ));
        }
        parts.join("\n\n")
    }

    /// Launch validation agents to verify each finding, filtering false positives.
    pub async fn validate(
        &self,
        findings: Vec<String>,
        context: &str,
    ) -> Vec<String> {
        use tokio::task::JoinSet;

        let mut join_set = JoinSet::new();

        for finding in findings {
            let provider = self.provider.clone();
            let ctx = context.to_string();

            join_set.spawn(async move {
                let validation_prompt = format!(
                    "你是一个验证器。请评估以下发现是否是真实问题（非误报）。\n\n\
                     上下文:\n{}\n\n发现:\n{}\n\n\
                     请只输出 JSON: {{\"valid\": true/false, \"reason\": \"简要说明\"}}",
                    ctx.chars().take(3000).collect::<String>(),
                    finding
                );

                let req = ChatRequest {
                    messages: vec![ChatMessage {
                        role: MessageRole::User,
                        content: validation_prompt,
                        name: None,
                        tool_calls: None,
                        tool_call_id: None,
                    }],
                    tools: None,
                    temperature: 0.1,
                    max_tokens: Some(300),
                };

                match provider.chat(req).await {
                    Ok(resp) => {
                        let content = resp.message.content.to_lowercase();
                        if content.contains("\"valid\": true") || content.contains("\"valid\":true") {
                            Some(finding)
                        } else {
                            tracing::info!("[Orchestrator] Validation rejected finding as false positive");
                            None
                        }
                    }
                    Err(e) => {
                        tracing::warn!("[Orchestrator] Validation failed: {}, keeping finding", e);
                        Some(finding)
                    }
                }
            });
        }

        let mut validated = Vec::new();
        while let Some(res) = join_set.join_next().await {
            if let Ok(Some(finding)) = res {
                validated.push(finding);
            }
        }
        validated
    }
}

/// Run a single sub-agent: multi-round tool loop with limited rounds and no streaming.
async fn run_sub_agent(
    provider: &Arc<dyn ModelProvider>,
    cfg: &crate::config::AppConfig,
    base_system_prompt: &str,
    task: &SubAgentTask,
) -> String {
    let tools = if task.tools_enabled {
        Some(crate::tools::registry::ToolRegistry::new().to_tool_definitions())
    } else {
        None
    };

    let mut messages = vec![
        ChatMessage {
            role: MessageRole::System,
            content: format!(
                "{}\n\n你的角色: {}\n请专注完成以下任务，并返回你的发现和结论。",
                base_system_prompt, task.role
            ),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        },
        ChatMessage {
            role: MessageRole::User,
            content: task.prompt.clone(),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        },
    ];

    let file_roots: Vec<std::path::PathBuf> = cfg.tools.file_access.iter()
        .map(|s| std::path::PathBuf::from(shellexpand::tilde(s).to_string()))
        .collect();
    let file_ops = crate::tools::file_ops::FileOps::new(file_roots);
    let shell = crate::tools::shell::ShellExecutor::new(
        cfg.tools.shell_enabled,
        cfg.tools.shell_allowed_commands.clone(),
    );

    for round in 0..task.max_rounds {
        let req = ChatRequest {
            messages: messages.clone(),
            tools: if round < task.max_rounds - 1 { tools.clone() } else { None },
            temperature: 0.5,
            max_tokens: None,
        };

        let resp = match provider.chat(req).await {
            Ok(r) => r,
            Err(e) => return format!("Sub-agent model error: {}", e),
        };

        let tool_calls = resp.message.tool_calls.clone().unwrap_or_default();

        if tool_calls.is_empty() {
            return resp.message.content;
        }

        for tc in &tool_calls {
            let result = crate::commands::dispatch_tool(tc, &file_ops, &shell).await;
            messages.push(ChatMessage {
                role: MessageRole::Assistant,
                content: String::new(),
                name: None,
                tool_calls: Some(vec![tc.clone()]),
                tool_call_id: None,
            });
            messages.push(ChatMessage {
                role: MessageRole::Tool,
                content: result,
                name: Some(tc.name.clone()),
                tool_calls: None,
                tool_call_id: Some(tc.id.clone()),
            });
        }
    }

    messages.last()
        .map(|m| m.content.clone())
        .unwrap_or_else(|| "Sub-agent reached max rounds without conclusion.".to_string())
}
