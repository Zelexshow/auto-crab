use crate::config::AppConfig;
use crate::core::context::ContextManager;
use crate::models::provider::*;
use crate::models::ModelRouter;
use crate::security::approval::{ApprovalDecision, ApprovalGate, ApprovalResult};
use crate::security::audit::{AuditLogger, AuditSource, AuditStatus};
use crate::security::risk::RiskEngine;
use crate::tools::file_ops::FileOps;
use crate::tools::registry::ToolRegistry;
use crate::tools::shell::ShellExecutor;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct Agent {
    router: ModelRouter,
    context: Arc<Mutex<ContextManager>>,
    tools: ToolRegistry,
    file_ops: FileOps,
    shell: ShellExecutor,
    approval: Arc<ApprovalGate>,
    audit: Arc<AuditLogger>,
    config: AppConfig,
}

pub struct AgentRunResult {
    pub final_response: String,
    pub model_used: String,
    pub tool_calls_made: Vec<ToolCallRecord>,
    pub total_tokens: u32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolCallRecord {
    pub tool_name: String,
    pub arguments: String,
    pub result: String,
    pub approved: bool,
}

impl Agent {
    pub fn new(config: AppConfig, data_dir: PathBuf) -> anyhow::Result<Self> {
        let router = ModelRouter::from_config(&config)?;
        let risk_engine = RiskEngine::new(config.security.risk_overrides.clone());
        let approval = Arc::new(ApprovalGate::new(risk_engine));
        let audit = Arc::new(AuditLogger::new(data_dir));

        let file_roots: Vec<PathBuf> = config
            .tools
            .file_access
            .iter()
            .map(|s| PathBuf::from(shellexpand::tilde(s).to_string()))
            .collect();

        Ok(Self {
            router,
            context: Arc::new(Mutex::new(ContextManager::new(
                config.agent.max_context_tokens,
            ))),
            tools: ToolRegistry::new(),
            file_ops: FileOps::new(file_roots),
            shell: ShellExecutor::new(
                config.tools.shell_enabled,
                config.tools.shell_allowed_commands.clone(),
            ),
            approval,
            audit,
            config,
        })
    }

    /// Run the agent loop: send messages to model, handle tool calls, iterate
    pub async fn run(&self, user_message: &str) -> anyhow::Result<AgentRunResult> {
        let mut ctx = self.context.lock().await;

        ctx.add_message(ChatMessage {
            role: MessageRole::User,
            content: user_message.to_string(),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        });

        let mut tool_records = Vec::new();
        let mut total_tokens = 0u32;
        let mut model_used = String::new();
        let max_iterations = 10;

        for _iteration in 0..max_iterations {
            let messages = ctx.build_messages(&self.config.agent.system_prompt);
            let tool_defs = self.tools.to_openai_tools();

            let request = ChatRequest {
                messages,
                tools: if tool_defs.is_empty() {
                    None
                } else {
                    Some(self.tools.to_tool_definitions())
                },
                temperature: 0.7,
                max_tokens: None,
            };

            let response = self.router.chat_with_fallback(request).await?;
            model_used = response.model.clone();

            if let Some(usage) = &response.usage {
                total_tokens += usage.total_tokens;
            }

            if let Some(ref tool_calls) = response.message.tool_calls {
                if !tool_calls.is_empty() {
                    ctx.add_message(response.message.clone());

                    for tc in tool_calls {
                        let record = self.execute_tool_call(tc).await;
                        tool_records.push(record.clone());

                        ctx.add_message(ChatMessage {
                            role: MessageRole::Tool,
                            content: record.result.clone(),
                            name: Some(tc.name.clone()),
                            tool_calls: None,
                            tool_call_id: Some(tc.id.clone()),
                        });
                    }
                    continue;
                }
            }

            let final_text = response.message.content.clone();
            ctx.add_message(response.message);

            return Ok(AgentRunResult {
                final_response: final_text,
                model_used,
                tool_calls_made: tool_records,
                total_tokens,
            });
        }

        Ok(AgentRunResult {
            final_response: "达到最大迭代次数，已停止执行。".into(),
            model_used,
            tool_calls_made: tool_records,
            total_tokens,
        })
    }

    async fn execute_tool_call(&self, tc: &ToolCall) -> ToolCallRecord {
        let tool_spec = self.tools.get(&tc.name);
        let operation_type = tool_spec
            .map(|s| s.operation_type.as_str())
            .unwrap_or("unknown");

        let approval_result = self
            .approval
            .request(
                operation_type,
                &format!(
                    "{}({})",
                    tc.name,
                    &tc.arguments[..tc.arguments.len().min(100)]
                ),
                serde_json::json!({ "tool": tc.name, "args": tc.arguments }),
            )
            .await;

        match approval_result {
            Err(e) => {
                let _ = self
                    .audit
                    .log(
                        operation_type,
                        crate::config::RiskLevel::Forbidden,
                        AuditStatus::Blocked,
                        &format!("{}: {}", tc.name, e),
                        AuditSource::Local,
                    )
                    .await;

                ToolCallRecord {
                    tool_name: tc.name.clone(),
                    arguments: tc.arguments.clone(),
                    result: format!("操作被禁止: {}", e),
                    approved: false,
                }
            }
            Ok(ApprovalResult::AutoApproved) => {
                let result = self.do_tool_execution(tc).await;
                let _ = self
                    .audit
                    .log(
                        operation_type,
                        crate::config::RiskLevel::Safe,
                        AuditStatus::AutoApproved,
                        &tc.name,
                        AuditSource::Local,
                    )
                    .await;

                ToolCallRecord {
                    tool_name: tc.name.clone(),
                    arguments: tc.arguments.clone(),
                    result,
                    approved: true,
                }
            }
            Ok(ApprovalResult::Pending {
                approval: _,
                receiver,
            }) => match tokio::time::timeout(std::time::Duration::from_secs(300), receiver).await {
                Ok(Ok(ApprovalDecision::Approved)) => {
                    let result = self.do_tool_execution(tc).await;
                    let _ = self
                        .audit
                        .log(
                            operation_type,
                            crate::config::RiskLevel::Moderate,
                            AuditStatus::Approved,
                            &tc.name,
                            AuditSource::Local,
                        )
                        .await;

                    ToolCallRecord {
                        tool_name: tc.name.clone(),
                        arguments: tc.arguments.clone(),
                        result,
                        approved: true,
                    }
                }
                Ok(Ok(ApprovalDecision::Rejected { reason })) => {
                    let _ = self
                        .audit
                        .log(
                            operation_type,
                            crate::config::RiskLevel::Moderate,
                            AuditStatus::Rejected,
                            &format!("{}: {}", tc.name, reason),
                            AuditSource::Local,
                        )
                        .await;

                    ToolCallRecord {
                        tool_name: tc.name.clone(),
                        arguments: tc.arguments.clone(),
                        result: format!("操作被用户拒绝: {}", reason),
                        approved: false,
                    }
                }
                _ => ToolCallRecord {
                    tool_name: tc.name.clone(),
                    arguments: tc.arguments.clone(),
                    result: "操作审批超时（5分钟）".into(),
                    approved: false,
                },
            },
        }
    }

    async fn do_tool_execution(&self, tc: &ToolCall) -> String {
        let args: serde_json::Value =
            serde_json::from_str(&tc.arguments).unwrap_or(serde_json::Value::Null);

        match tc.name.as_str() {
            "read_file" => {
                let path = args["path"].as_str().unwrap_or("");
                match self.file_ops.read_file(path).await {
                    Ok(content) => {
                        if content.len() > 10000 {
                            format!(
                                "{}...\n[文件内容截断，共 {} 字符]",
                                &content[..10000],
                                content.len()
                            )
                        } else {
                            content
                        }
                    }
                    Err(e) => format!("读取文件失败: {}", e),
                }
            }
            "write_file" => {
                let path = args["path"].as_str().unwrap_or("");
                let content = args["content"].as_str().unwrap_or("");
                match self.file_ops.write_file(path, content).await {
                    Ok(()) => format!("文件已写入: {}", path),
                    Err(e) => format!("写入文件失败: {}", e),
                }
            }
            "list_directory" => {
                let path = args["path"].as_str().unwrap_or(".");
                match self.file_ops.list_directory(path).await {
                    Ok(entries) => {
                        let lines: Vec<String> = entries
                            .iter()
                            .map(|e| {
                                if e.is_dir {
                                    format!("📁 {}/", e.name)
                                } else {
                                    format!("📄 {} ({} bytes)", e.name, e.size)
                                }
                            })
                            .collect();
                        lines.join("\n")
                    }
                    Err(e) => format!("列目录失败: {}", e),
                }
            }
            "execute_shell" => {
                let command = args["command"].as_str().unwrap_or("");
                let working_dir = args["working_directory"].as_str();
                match self.shell.execute(command, working_dir).await {
                    Ok(output) => {
                        let mut result = String::new();
                        if !output.stdout.is_empty() {
                            result.push_str(&output.stdout);
                        }
                        if !output.stderr.is_empty() {
                            if !result.is_empty() {
                                result.push('\n');
                            }
                            result.push_str("[stderr] ");
                            result.push_str(&output.stderr);
                        }
                        result.push_str(&format!("\n[exit code: {}]", output.exit_code));
                        result
                    }
                    Err(e) => format!("命令执行失败: {}", e),
                }
            }
            "search_web" => {
                let query = args["query"].as_str().unwrap_or("");
                format!("网络搜索功能暂未接入，查询: {}", query)
            }
            _ => format!("未知工具: {}", tc.name),
        }
    }
}
