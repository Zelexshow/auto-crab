use crate::config::AppConfig;
use crate::models::provider::*;
use crate::security::audit::{AuditLogger, AuditSource, AuditStatus};
use crate::security::risk::RiskEngine;
use crate::tools::file_ops::FileOps;
use crate::tools::registry::ToolRegistry;
use crate::tools::shell::ShellExecutor;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Emitter;

// ── Tool action result (replaces magic strings) ─────────────────────

pub enum ToolAction {
    Direct(String),
    AnalyzeScreen { image_path: String, prompt: String },
    AnalyzeAndAct { task: String, max_steps: usize },
    WechatReply { contact: String, message: String },
}

// ── Event sink trait ────────────────────────────────────────────────

#[async_trait::async_trait]
pub trait EventSink: Send + Sync {
    fn on_thinking(&self, round: u32, stream_id: &str);
    fn on_thinking_done(&self, round: u32, stream_id: &str);
    fn on_tool_call(&self, step_id: &str, tool: &str, args: &str, status: &str, stream_id: &str);
    fn on_tool_result(&self, step_id: &str, tool: &str, result: &str, status: &str, stream_id: &str);
    fn on_plan_update(&self, plan_text: &str, stream_id: &str);
    fn on_stream_delta(&self, delta: &str, stream_id: &str);
    fn on_stream_end(&self, stream_id: &str);
    fn on_final_answer(&self, content: &str, stream_id: &str);
    fn on_error(&self, error: &str, stream_id: &str);
    fn on_done(&self, stream_id: &str);
    async fn request_approval(&self, tool: &str, args: &str, risk: &str, stream_id: &str, step_id: &str) -> bool;
    /// Whether this sink needs streaming UX. Remote sinks (Feishu etc.) return false
    /// to skip redundant LLM calls when content is already available.
    fn needs_streaming(&self) -> bool { true }
}

// ── Agent engine config ─────────────────────────────────────────────

pub struct AgentConfig {
    pub max_rounds: usize,
    pub tools_enabled: bool,
    pub audit: Option<Arc<AuditLogger>>,
    pub audit_source: AuditSource,
    pub memory: Option<Arc<super::long_memory::LongTermMemory>>,
    pub planning_enabled: bool,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_rounds: 8,
            tools_enabled: true,
            audit: None,
            audit_source: AuditSource::Local,
            memory: None,
            planning_enabled: true,
        }
    }
}

// ── Agent engine ────────────────────────────────────────────────────

pub struct AgentEngine {
    provider: Arc<dyn ModelProvider>,
    file_ops: FileOps,
    shell: ShellExecutor,
    tool_defs: Vec<ToolDefinition>,
    cfg: AppConfig,
    mcp_client: Option<Arc<crate::mcp::client::McpClientManager>>,
}

impl AgentEngine {
    pub fn from_config(cfg: &AppConfig) -> anyhow::Result<Self> {
        let router = crate::models::ModelRouter::from_config(cfg)?;
        let provider = router
            .get_primary()
            .ok_or_else(|| anyhow::anyhow!("No model provider configured"))?;

        let file_roots: Vec<PathBuf> = cfg
            .tools
            .file_access
            .iter()
            .map(|s| PathBuf::from(shellexpand::tilde(s).to_string()))
            .collect();

        Ok(Self {
            provider,
            file_ops: FileOps::new(file_roots),
            shell: ShellExecutor::new(
                cfg.tools.shell_enabled,
                cfg.tools.shell_allowed_commands.clone(),
            ),
            tool_defs: ToolRegistry::new().to_tool_definitions(),
            cfg: cfg.clone(),
            mcp_client: None,
        })
    }

    /// Attach a shared MCP client manager to this engine. The engine will
    /// merge MCP tools into its tool definitions and route calls accordingly.
    pub fn with_mcp_client(mut self, client: Arc<crate::mcp::client::McpClientManager>) -> Self {
        self.mcp_client = Some(client);
        self
    }

    /// Refresh tool definitions, merging builtin tools with MCP-discovered tools.
    pub async fn refresh_tool_defs(&mut self) {
        let mut defs = ToolRegistry::new().to_tool_definitions();
        if let Some(ref mcp) = self.mcp_client {
            let mcp_defs = mcp.get_tool_definitions().await;
            tracing::info!("[Engine] Merging {} MCP tools into {} builtins", mcp_defs.len(), defs.len());
            defs.extend(mcp_defs);
        }
        self.tool_defs = defs;
    }

    pub async fn run(
        &self,
        messages: Vec<ChatMessage>,
        stream_id: &str,
        sink: &dyn EventSink,
        agent_cfg: &AgentConfig,
    ) -> String {
        let mut messages = messages;

        // RAG: recall relevant memories (with timeout to avoid blocking)
        if let Some(ref mem) = agent_cfg.memory {
            if let Some(user_msg) = messages.last() {
                let msg_len = user_msg.content.chars().count();
                // Skip memory recall for very short messages (greetings, commands, etc.)
                if user_msg.role == MessageRole::User && msg_len > 10 {
                    let recall_start = std::time::Instant::now();
                    // 3s timeout — don't let slow embedding API block the conversation
                    match tokio::time::timeout(
                        std::time::Duration::from_secs(3),
                        mem.recall(&user_msg.content, Some(3))
                    ).await {
                        Ok(Ok(recalls)) if !recalls.is_empty() => {
                            let memory_context = format!(
                                "以下是与当前对话相关的历史记忆（供参考，不要逐条复述）：\n{}",
                                recalls.join("\n")
                            );
                            messages.insert(1, ChatMessage {
                                role: MessageRole::System,
                                content: memory_context,
                                name: None, tool_calls: None, tool_call_id: None,
                            });
                            tracing::info!("[Memory] Injected {} memories in {:.1}s", recalls.len(), recall_start.elapsed().as_secs_f64());
                        }
                        Ok(Ok(_)) => {
                            tracing::info!("[Memory] No relevant memories (took {:.1}s)", recall_start.elapsed().as_secs_f64());
                        }
                        Ok(Err(e)) => {
                            tracing::warn!("[Memory] Recall error (took {:.1}s): {}", recall_start.elapsed().as_secs_f64(), e);
                        }
                        Err(_) => {
                            tracing::warn!("[Memory] Recall timed out after 3s, skipping");
                        }
                    }
                }
            }
        }

        let user_input = messages.last()
            .filter(|m| m.role == MessageRole::User)
            .map(|m| m.content.clone())
            .unwrap_or_default();

        // Check if this task needs planning
        if agent_cfg.planning_enabled && super::planner::should_plan(&user_input) {
            tracing::info!("[Engine] Complex task detected, activating planner");
            return self.run_with_plan(messages, stream_id, sink, agent_cfg, &user_input).await;
        }

        // Direct execution (no planning needed)
        let result = self.run_direct(messages, stream_id, sink, agent_cfg).await;
        self.save_to_memory(&user_input, &result, agent_cfg).await;
        result
    }

    /// Rough char-count estimate; trim tool_result messages if total exceeds threshold.
    fn trim_context_if_needed(messages: &mut Vec<ChatMessage>, max_chars: usize) {
        let total: usize = messages.iter().map(|m| m.content.len()).sum();
        if total <= max_chars { return; }
        tracing::warn!("[Engine] Context too large ({} chars, max {}), trimming tool results", total, max_chars);
        for msg in messages.iter_mut() {
            if msg.role == MessageRole::Tool && msg.content.len() > 2000 {
                let truncated: String = msg.content.chars().take(1500).collect();
                msg.content = format!("{}...\n[结果已截断，原始长度: {} 字符]", truncated, msg.content.len());
            }
        }
    }

    /// Direct Agent loop with full streaming support (main path for simple tasks).
    async fn run_direct(
        &self,
        messages: Vec<ChatMessage>,
        stream_id: &str,
        sink: &dyn EventSink,
        agent_cfg: &AgentConfig,
    ) -> String {
        let mut messages = messages;

        for round in 0..=agent_cfg.max_rounds {
            let use_tools = round < agent_cfg.max_rounds && agent_cfg.tools_enabled && self.cfg.tools.shell_enabled;

            // Final round with tools disabled: guide the model to produce its answer
            // directly instead of attempting tool calls (which causes DSML on DeepSeek).
            if !use_tools && round > 0 {
                messages.push(ChatMessage {
                    role: MessageRole::System,
                    content: "你已经收集了足够的数据。请立即基于以上已获取的所有数据和信息生成完整的最终回答，不要再尝试调用任何工具。直接输出分析结论。".to_string(),
                    name: None, tool_calls: None, tool_call_id: None,
                });
            }

            sink.on_thinking(round as u32, stream_id);

            Self::trim_context_if_needed(&mut messages, 120_000);

            let req = ChatRequest {
                messages: messages.clone(),
                tools: if use_tools { Some(self.tool_defs.clone()) } else { None },
                temperature: 0.7,
                max_tokens: None,
            };

            let msg_chars: usize = messages.iter().map(|m| m.content.len()).sum();
            tracing::info!("[Engine] Round {} - calling model with {} messages ({} chars), tools: {}", round, messages.len(), msg_chars, use_tools);

            let resp = {
                let mut last_err = String::new();
                let mut result = None;
                for attempt in 0..=1 {
                    match self.provider.chat(req.clone()).await {
                        Ok(r) => { result = Some(r); break; }
                        Err(e) => {
                            last_err = e.to_string();
                            if attempt == 0 {
                                tracing::warn!("[Engine] Model call failed (attempt 1), retrying in 2s: {}", &last_err[..last_err.len().min(200)]);
                                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                            }
                        }
                    }
                }
                match result {
                    Some(r) => r,
                    None => {
                        tracing::error!("[Engine] Model call FAILED after 2 attempts: {}", last_err);
                        sink.on_error(&last_err, stream_id);
                        return format!("模型调用失败: {}", last_err);
                    }
                }
            };

            tracing::info!("[Engine] Round {} - model responded, tool_calls: {}, content_len: {}",
                round,
                resp.message.tool_calls.as_ref().map(|t| t.len()).unwrap_or(0),
                resp.message.content.len()
            );

            sink.on_thinking_done(round as u32, stream_id);

            let tool_calls = resp.message.tool_calls.clone().unwrap_or_default();

            if !tool_calls.is_empty() {
                for tc in &tool_calls {
                    let step_id = uuid::Uuid::new_v4().to_string();
                    let result = self.execute_tool_call(tc, sink, stream_id, &step_id, agent_cfg).await;

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
                continue;
            }

            let content = resp.message.content;
            if content.contains("DSML") && content.contains("function_calls") {
                tracing::warn!("DSML detected, retrying without tools");
                if !sink.needs_streaming() {
                    let clean = Self::build_clean_messages(&messages);
                    let retry_req = ChatRequest {
                        messages: clean,
                        tools: None,
                        temperature: 0.7,
                        max_tokens: None,
                    };
                    if let Ok(retry_resp) = self.provider.chat(retry_req).await {
                        let c = retry_resp.message.content;
                        if !c.is_empty() && !c.contains("DSML") {
                            sink.on_final_answer(&c, stream_id);
                            sink.on_done(stream_id);
                            return c;
                        }
                    }
                    let fallback = "抱歉，AI 返回了异常格式。请重试一次。".to_string();
                    sink.on_final_answer(&fallback, stream_id);
                    sink.on_done(stream_id);
                    return fallback;
                }
                return self.stream_final_answer(&messages, sink, stream_id).await;
            }

            if !content.is_empty() {
                if !sink.needs_streaming() {
                    tracing::info!("[Engine] Remote sink: returning content directly (skipping stream_final_answer)");
                    sink.on_final_answer(&content, stream_id);
                    sink.on_done(stream_id);
                    return content;
                }
                return self.stream_final_answer(&messages, sink, stream_id).await;
            }

            sink.on_final_answer(&content, stream_id);
            sink.on_done(stream_id);
            return content;
        }

        let msg = "已达到最大工具调用轮次，操作停止。".to_string();
        sink.on_final_answer(&msg, stream_id);
        sink.on_done(stream_id);
        msg
    }

    async fn run_with_plan(
        &self,
        base_messages: Vec<ChatMessage>,
        stream_id: &str,
        sink: &dyn EventSink,
        agent_cfg: &AgentConfig,
        user_input: &str,
    ) -> String {
        use super::planner::*;

        let planner = Planner::new(self.provider.clone());

        sink.on_plan_update("📋 正在分解任务...", stream_id);

        let mut plan = match planner.plan(user_input).await {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("[Planner] Plan generation failed: {}, falling back to direct execution", e);
                return self.run_simple(base_messages, stream_id, sink, &AgentConfig {
                    planning_enabled: false,
                    max_rounds: agent_cfg.max_rounds,
                    tools_enabled: agent_cfg.tools_enabled,
                    audit: agent_cfg.audit.clone(),
                    audit_source: agent_cfg.audit_source.clone(),
                    memory: agent_cfg.memory.clone(),
                }).await;
            }
        };

        tracing::info!("[Planner] Plan created with {} steps", plan.steps.len());
        sink.on_plan_update(&plan.progress_text(), stream_id);

        let mut step_results: Vec<String> = Vec::new();
        let mut retries = 0;
        let max_retries = 2;

        while !plan.is_complete {
            let step = match plan.current() {
                Some(s) => s.clone(),
                None => break,
            };

            plan.mark_current(StepStatus::Running, None);
            sink.on_plan_update(&plan.progress_text(), stream_id);

            // Build messages for this step with context from previous steps
            let mut step_messages = base_messages.clone();
            if !step_results.is_empty() {
                let context = format!(
                    "你正在执行一个多步骤任务。以下是之前步骤的结果：\n{}\n\n现在请执行下一步：{}",
                    step_results.iter().enumerate()
                        .map(|(i, r)| format!("步骤{}: {}", i + 1, r.chars().take(800).collect::<String>()))
                        .collect::<Vec<_>>()
                        .join("\n"),
                    step.description
                );
                step_messages.push(ChatMessage {
                    role: MessageRole::User,
                    content: context,
                    name: None, tool_calls: None, tool_call_id: None,
                });
            } else {
                // First step: rewrite user message to focus on this step
                if let Some(last) = step_messages.last_mut() {
                    if last.role == MessageRole::User {
                        last.content = format!("{}（当前执行步骤1：{}）", last.content, step.description);
                    }
                }
            }

            // Execute this step with limited rounds
            let step_cfg = AgentConfig {
                max_rounds: 4,
                tools_enabled: agent_cfg.tools_enabled,
                audit: agent_cfg.audit.clone(),
                audit_source: agent_cfg.audit_source.clone(),
                memory: None,
                planning_enabled: false,
            };

            let step_result = self.run_simple(step_messages, stream_id, sink, &step_cfg).await;

            // Reflect on the result
            let decision = match planner.reflect(&plan, &step_result).await {
                Ok(d) => d,
                Err(e) => {
                    tracing::warn!("[Planner] Reflect failed: {}, continuing", e);
                    ReflectDecision::Continue
                }
            };

            match decision {
                ReflectDecision::Continue => {
                    plan.mark_current(StepStatus::Done, Some(step_result.clone()));
                    step_results.push(step_result);
                    plan.advance();
                    retries = 0;
                }
                ReflectDecision::Retry { reason } => {
                    retries += 1;
                    if retries > max_retries {
                        tracing::warn!("[Planner] Max retries reached for step {}", step.id);
                        plan.mark_current(StepStatus::Failed, Some(format!("重试{}次仍失败: {}", max_retries, reason)));
                        step_results.push(format!("(步骤失败: {})", reason));
                        plan.advance();
                        retries = 0;
                    } else {
                        tracing::info!("[Planner] Retrying step {}: {}", step.id, reason);
                    }
                }
                ReflectDecision::Skip { reason } => {
                    plan.mark_current(StepStatus::Skipped, Some(reason.clone()));
                    step_results.push(format!("(已跳过: {})", reason));
                    plan.advance();
                    retries = 0;
                }
                ReflectDecision::Abort { reason } => {
                    plan.mark_current(StepStatus::Failed, Some(reason.clone()));
                    tracing::info!("[Planner] Aborting plan: {}", reason);
                    sink.on_plan_update(&format!("{}\n\n⚠️ 任务中止: {}", plan.progress_text(), reason), stream_id);
                    break;
                }
                ReflectDecision::ReviseAndContinue { revised_step } => {
                    plan.mark_current(StepStatus::Done, Some(step_result.clone()));
                    step_results.push(step_result);
                    plan.advance();
                    // Revise the next step description if possible
                    if let Some(next) = plan.steps.get_mut(plan.current_step) {
                        tracing::info!("[Planner] Revised step {}: {}", next.id, revised_step);
                        next.description = revised_step;
                    }
                    retries = 0;
                }
            }

            sink.on_plan_update(&plan.progress_text(), stream_id);
        }

        // Generate final summary with full step data to prevent hallucination
        let summary_prompt = format!(
            "你执行了以下多步骤任务：\n目标: {}\n\n各步骤结果:\n{}\n\n请基于上述步骤结果给用户一个完整的总结报告。\n重要：只使用上面步骤中返回的实际数据，不要编造任何数据来源、时间、价格或数值。如果某个步骤失败或数据不完整，请如实说明。",
            plan.goal,
            step_results.iter().enumerate()
                .map(|(i, r)| format!("步骤{}: {}", i + 1, r.chars().take(1500).collect::<String>()))
                .collect::<Vec<_>>()
                .join("\n")
        );

        let summary_messages = vec![
            ChatMessage {
                role: MessageRole::System,
                content: self.cfg.agent.system_prompt.clone(),
                name: None, tool_calls: None, tool_call_id: None,
            },
            ChatMessage {
                role: MessageRole::User,
                content: summary_prompt,
                name: None, tool_calls: None, tool_call_id: None,
            },
        ];

        let final_answer = self.stream_final_answer(&summary_messages, sink, stream_id).await;
        self.save_to_memory(user_input, &final_answer, agent_cfg).await;
        final_answer
    }

    /// The core Agent loop without planning (used by both direct execution and per-step execution).
    async fn run_simple(
        &self,
        messages: Vec<ChatMessage>,
        stream_id: &str,
        sink: &dyn EventSink,
        agent_cfg: &AgentConfig,
    ) -> String {
        let mut messages = messages;

        for round in 0..=agent_cfg.max_rounds {
            let use_tools = round < agent_cfg.max_rounds && agent_cfg.tools_enabled && self.cfg.tools.shell_enabled;

            sink.on_thinking(round as u32, stream_id);

            Self::trim_context_if_needed(&mut messages, 120_000);

            let req = ChatRequest {
                messages: messages.clone(),
                tools: if use_tools { Some(self.tool_defs.clone()) } else { None },
                temperature: 0.7,
                max_tokens: None,
            };

            let msg_chars: usize = messages.iter().map(|m| m.content.len()).sum();
            tracing::info!("[Engine] Round {} - calling model with {} messages ({} chars), tools: {}", round, messages.len(), msg_chars, use_tools);

            let resp = {
                let mut last_err = String::new();
                let mut result = None;
                for attempt in 0..=1 {
                    match self.provider.chat(req.clone()).await {
                        Ok(r) => { result = Some(r); break; }
                        Err(e) => {
                            last_err = e.to_string();
                            if attempt == 0 {
                                tracing::warn!("[Engine] Model call failed (attempt 1), retrying in 2s: {}", &last_err[..last_err.len().min(200)]);
                                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                            }
                        }
                    }
                }
                match result {
                    Some(r) => r,
                    None => {
                        tracing::error!("[Engine] Model call FAILED after 2 attempts: {}", last_err);
                        return format!("模型调用失败: {}", last_err);
                    }
                }
            };

            sink.on_thinking_done(round as u32, stream_id);

            let tool_calls = resp.message.tool_calls.clone().unwrap_or_default();
            if !tool_calls.is_empty() {
                for tc in &tool_calls {
                    let step_id = uuid::Uuid::new_v4().to_string();
                    let result = self.execute_tool_call(tc, sink, stream_id, &step_id, agent_cfg).await;
                    messages.push(ChatMessage {
                        role: MessageRole::Assistant, content: String::new(), name: None,
                        tool_calls: Some(vec![tc.clone()]), tool_call_id: None,
                    });
                    messages.push(ChatMessage {
                        role: MessageRole::Tool, content: result, name: Some(tc.name.clone()),
                        tool_calls: None, tool_call_id: Some(tc.id.clone()),
                    });
                }
                continue;
            }

            let content = resp.message.content;
            if content.contains("DSML") && content.contains("function_calls") {
                return content;
            }

            return content;
        }

        "已达到最大工具调用轮次".to_string()
    }

    async fn save_to_memory(&self, user_input: &str, answer: &str, agent_cfg: &AgentConfig) {
        if let Some(ref mem) = agent_cfg.memory {
            if user_input.len() > 10 && answer.len() > 20 {
                let summary = format!("用户: {}\nAI: {}", 
                    user_input.chars().take(200).collect::<String>(),
                    answer.chars().take(300).collect::<String>()
                );
                let _ = mem.store(&summary, "conversation").await;
            }
        }
    }

    async fn execute_tool_call(
        &self,
        tc: &ToolCall,
        sink: &dyn EventSink,
        stream_id: &str,
        step_id: &str,
        agent_cfg: &AgentConfig,
    ) -> String {
        let op = crate::commands::tool_operation_type(&tc.name);
        let risk_engine = RiskEngine::new(HashMap::new());
        let risk = risk_engine.assess(op);

        // Forbidden
        if risk == crate::config::RiskLevel::Forbidden {
            let msg = format!("操作被禁止: {}", tc.name);
            sink.on_tool_result(step_id, &tc.name, &msg, "error", stream_id);
            if let Some(ref a) = agent_cfg.audit {
                let _ = a.log(op, risk, AuditStatus::Blocked, &tc.name, agent_cfg.audit_source.clone()).await;
            }
            return msg;
        }

        // Approval check
        let is_safe_shell = tc.name == "execute_shell" && crate::commands::is_readonly_shell_command_pub(&tc.arguments);
        let needs_approval = !is_safe_shell && matches!(
            risk,
            crate::config::RiskLevel::Moderate | crate::config::RiskLevel::Dangerous
        );

        if needs_approval {
            let risk_str = match risk {
                crate::config::RiskLevel::Dangerous => "dangerous",
                _ => "moderate",
            };
            let approved = sink.request_approval(&tc.name, &tc.arguments, risk_str, stream_id, step_id).await;
            if !approved {
                let msg = "操作被用户拒绝或审批超时".to_string();
                sink.on_tool_result(step_id, &tc.name, &msg, "error", stream_id);
                return msg;
            }
        }

        // Execute
        sink.on_tool_call(step_id, &tc.name, &tc.arguments, "running", stream_id);
        let tool_start = std::time::Instant::now();

        // Route MCP tools to the MCP client manager
        let raw_result = if let Some(ref mcp) = self.mcp_client {
            if mcp.is_mcp_tool(&tc.name).await {
                match mcp.call_tool(&tc.name, &tc.arguments).await {
                    Ok(r) => r,
                    Err(e) => format!("MCP tool '{}' failed: {}", tc.name, e),
                }
            } else {
                crate::commands::dispatch_tool(tc, &self.file_ops, &self.shell).await
            }
        } else {
            crate::commands::dispatch_tool(tc, &self.file_ops, &self.shell).await
        };
        let result = self.resolve_tool_action(&raw_result).await;
        let tool_ms = tool_start.elapsed().as_millis() as u64;
        crate::commands::record_perf_event("tool_call", &tc.name, tool_ms);

        // Audit
        if let Some(ref a) = agent_cfg.audit {
            let details: String = format!("{}({})", tc.name, tc.arguments.chars().take(100).collect::<String>());
            let _ = a.log(op, risk, AuditStatus::AutoApproved, &details, agent_cfg.audit_source.clone()).await;
        }

        let result_preview: String = result.chars().take(300).collect();
        tracing::info!("[Engine] Tool result ({} ms): {}", tool_ms, result_preview);
        sink.on_tool_result(step_id, &tc.name, &result, "done", stream_id);
        result
    }

    /// Strip tool protocol messages and inject tool results as plain-text context.
    /// Produces a clean message list that won't trigger DSML from the model.
    fn build_clean_messages(messages: &[ChatMessage]) -> Vec<ChatMessage> {
        let mut tool_results: Vec<String> = Vec::new();
        for m in messages {
            if m.role == MessageRole::Tool {
                let tool_name = m.name.as_deref().unwrap_or("tool");
                let preview: String = m.content.chars().take(1500).collect();
                tool_results.push(format!("[{}] {}", tool_name, preview));
            }
        }

        let mut clean: Vec<ChatMessage> = messages.iter()
            .filter(|m| {
                if m.role == MessageRole::Tool { return false; }
                if m.role == MessageRole::Assistant && m.content.is_empty() && m.tool_calls.is_some() { return false; }
                if m.role == MessageRole::Assistant && !m.content.is_empty() && m.tool_calls.is_none() { return false; }
                true
            })
            .cloned()
            .collect();

        if !tool_results.is_empty() {
            let context = format!(
                "以下是工具调用返回的实际数据，请基于这些数据回答用户，不要编造：\n{}",
                tool_results.join("\n")
            );
            clean.insert(1.min(clean.len()), ChatMessage {
                role: MessageRole::System,
                content: context,
                name: None, tool_calls: None, tool_call_id: None,
            });
        }
        clean
    }

    async fn stream_final_answer(
        &self,
        messages: &[ChatMessage],
        sink: &dyn EventSink,
        stream_id: &str,
    ) -> String {
        use futures::StreamExt;

        let clean_messages = Self::build_clean_messages(messages);

        let req = ChatRequest {
            messages: clean_messages,
            tools: None,
            temperature: 0.7,
            max_tokens: None,
        };

        match self.provider.chat_stream(req).await {
            Ok(mut stream) => {
                let mut full_content = String::new();
                while let Some(chunk_result) = stream.next().await {
                    match chunk_result {
                        Ok(chunk) => {
                            if !chunk.delta.is_empty() {
                                sink.on_stream_delta(&chunk.delta, stream_id);
                                full_content.push_str(&chunk.delta);
                            }
                            if chunk.finish_reason.is_some() {
                                break;
                            }
                        }
                        Err(e) => {
                            tracing::warn!("[Engine] Stream error: {}", e);
                            break;
                        }
                    }
                }
                sink.on_stream_end(stream_id);
                sink.on_done(stream_id);

                if full_content.contains("DSML") && full_content.contains("function_calls") {
                    tracing::error!("DSML in streamed output");
                    let fallback = "抱歉，AI 返回了异常格式。请重试一次。".to_string();
                    return fallback;
                }

                full_content
            }
            Err(e) => {
                tracing::error!("[Engine] Stream init failed: {}, falling back to non-stream", e);
                let clean_fallback = Self::build_clean_messages(messages);
                match self.provider.chat(ChatRequest {
                    messages: clean_fallback,
                    tools: None,
                    temperature: 0.7,
                    max_tokens: None,
                }).await {
                    Ok(resp) => {
                        let content = resp.message.content;
                        sink.on_final_answer(&content, stream_id);
                        sink.on_done(stream_id);
                        content
                    }
                    Err(e2) => {
                        let msg = format!("模型调用失败: {}", e2);
                        sink.on_error(&msg, stream_id);
                        msg
                    }
                }
            }
        }
    }

    async fn resolve_tool_action(&self, raw: &str) -> String {
        if raw.starts_with("__WECHAT_REPLY__:") {
            let rest = &raw["__WECHAT_REPLY__:".len()..];
            if let Some(sep) = rest.find("::") {
                let contact = &rest[..sep];
                let message = &rest[sep + 2..];
                let task = format!("在微信中找到联系人或群聊「{}」，点击进入聊天，在输入框中输入「{}」，然后按回车发送", contact, message);
                return crate::commands::execute_analyze_and_act(&self.cfg, &task, 5).await;
            }
        }
        if raw.starts_with("__ANALYZE_AND_ACT__:") {
            let rest = &raw["__ANALYZE_AND_ACT__:".len()..];
            if let Some(sep) = rest.find("::") {
                let max_steps: usize = rest[..sep].parse().unwrap_or(3);
                let task = &rest[sep + 2..];
                return crate::commands::execute_analyze_and_act(&self.cfg, task, max_steps).await;
            }
        }
        if raw.starts_with("__ANALYZE_SCREEN__:") {
            let rest = &raw["__ANALYZE_SCREEN__:".len()..];
            if let Some(sep) = rest.find("::") {
                let img_path = &rest[..sep];
                let question = &rest[sep + 2..];
                return match crate::commands::analyze_screenshot_with_prompt(&self.cfg, img_path, question).await {
                    Ok(analysis) => analysis,
                    Err(e) => format!("视觉分析失败: {}", e),
                };
            }
        }
        raw.to_string()
    }
}

// ── Build initial messages helper ───────────────────────────────────

pub fn build_messages(
    system_prompt: &str,
    history: &[ChatMessage],
    user_message: &str,
) -> Vec<ChatMessage> {
    let mut messages = vec![ChatMessage {
        role: MessageRole::System,
        content: system_prompt.to_string(),
        name: None,
        tool_calls: None,
        tool_call_id: None,
    }];
    messages.extend(history.iter().cloned());
    messages.push(ChatMessage {
        role: MessageRole::User,
        content: user_message.to_string(),
        name: None,
        tool_calls: None,
        tool_call_id: None,
    });
    messages
}

pub fn history_messages_to_chat(history: &[crate::commands::HistoryMessage]) -> Vec<ChatMessage> {
    history.iter().map(|h| {
        let role = match h.role.as_str() {
            "assistant" => MessageRole::Assistant,
            "system" => MessageRole::System,
            _ => MessageRole::User,
        };
        ChatMessage {
            role,
            content: h.content.clone(),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        }
    }).collect()
}

// ── TauriEventSink ──────────────────────────────────────────────────

pub struct TauriEventSink {
    emitter: tauri::AppHandle,
}

impl TauriEventSink {
    pub fn new(emitter: tauri::AppHandle) -> Self {
        Self { emitter }
    }
}

#[async_trait::async_trait]
impl EventSink for TauriEventSink {
    fn on_thinking(&self, round: u32, stream_id: &str) {
        let _ = self.emitter.emit("agent-step", serde_json::json!({
            "id": uuid::Uuid::new_v4().to_string(), "stream_id": stream_id,
            "type": "thinking",
            "content": if round == 0 { "正在分析你的请求..." } else { "继续思考中..." },
            "status": "running",
            "timestamp": chrono::Utc::now().timestamp_millis(),
        }));
    }

    fn on_thinking_done(&self, round: u32, stream_id: &str) {
        let _ = self.emitter.emit("agent-step", serde_json::json!({
            "id": uuid::Uuid::new_v4().to_string(), "stream_id": stream_id,
            "type": "thinking",
            "content": format!("第{}轮思考完成", round + 1),
            "status": "done",
            "timestamp": chrono::Utc::now().timestamp_millis(),
        }));
    }

    fn on_tool_call(&self, step_id: &str, tool: &str, args: &str, status: &str, stream_id: &str) {
        let _ = self.emitter.emit("agent-step", serde_json::json!({
            "id": step_id, "stream_id": stream_id,
            "type": "tool_call", "tool": tool,
            "content": args, "status": status,
            "timestamp": chrono::Utc::now().timestamp_millis(),
        }));
    }

    fn on_tool_result(&self, step_id: &str, tool: &str, result: &str, status: &str, stream_id: &str) {
        let _ = self.emitter.emit("agent-step", serde_json::json!({
            "id": step_id, "stream_id": stream_id,
            "type": "tool_result", "tool": tool,
            "content": result, "status": status,
            "timestamp": chrono::Utc::now().timestamp_millis(),
        }));
    }

    fn on_plan_update(&self, plan_text: &str, stream_id: &str) {
        let _ = self.emitter.emit("agent-step", serde_json::json!({
            "id": uuid::Uuid::new_v4().to_string(), "stream_id": stream_id,
            "type": "plan",
            "content": plan_text,
            "status": "running",
            "timestamp": chrono::Utc::now().timestamp_millis(),
        }));
    }

    fn on_stream_delta(&self, delta: &str, stream_id: &str) {
        let _ = self.emitter.emit("chat-stream-chunk", serde_json::json!({
            "stream_id": stream_id, "delta": delta, "done": false,
        }));
    }

    fn on_stream_end(&self, stream_id: &str) {
        let _ = self.emitter.emit("chat-stream-chunk", serde_json::json!({
            "stream_id": stream_id, "delta": "", "done": true,
        }));
    }

    fn on_final_answer(&self, content: &str, stream_id: &str) {
        let _ = self.emitter.emit("chat-stream-chunk", serde_json::json!({
            "stream_id": stream_id, "delta": content, "done": false,
        }));
        tokio::task::block_in_place(|| std::thread::sleep(std::time::Duration::from_millis(50)));
        let _ = self.emitter.emit("chat-stream-chunk", serde_json::json!({
            "stream_id": stream_id, "delta": "", "done": true,
        }));
    }

    fn on_error(&self, error: &str, stream_id: &str) {
        let _ = self.emitter.emit("chat-stream-error", serde_json::json!({
            "stream_id": stream_id, "error": error,
        }));
    }

    fn on_done(&self, stream_id: &str) {
        let _ = self.emitter.emit("agent-done", serde_json::json!({ "stream_id": stream_id }));
    }

    async fn request_approval(&self, tool: &str, args: &str, risk: &str, stream_id: &str, step_id: &str) -> bool {
        let approval_id = uuid::Uuid::new_v4().to_string();
        let _ = self.emitter.emit("approval-request", serde_json::json!({
            "id": &approval_id, "operation": tool, "risk_level": risk,
            "description": format!("{}({})", tool, args.chars().take(80).collect::<String>()),
        }));
        let _ = self.emitter.emit("agent-step", serde_json::json!({
            "id": step_id, "stream_id": stream_id,
            "type": "tool_call", "tool": tool,
            "content": format!("[等待审批] {}({})", tool, args.chars().take(60).collect::<String>()),
            "status": "blocked",
            "timestamp": chrono::Utc::now().timestamp_millis(),
        }));

        let (atx, arx) = tokio::sync::oneshot::channel::<bool>();
        {
            let mut pending = crate::commands::desktop_approvals().lock().unwrap();
            pending.insert(approval_id, atx);
        }

        tokio::time::timeout(std::time::Duration::from_secs(120), arx)
            .await
            .ok()
            .and_then(|r| r.ok())
            .unwrap_or(false)
    }
}

// ── StringCollectorSink ─────────────────────────────────────────────

pub struct StringCollectorSink;

#[async_trait::async_trait]
impl EventSink for StringCollectorSink {
    fn on_thinking(&self, round: u32, _stream_id: &str) {
        tracing::info!("[Remote] Round {} thinking...", round);
    }
    fn on_thinking_done(&self, round: u32, _stream_id: &str) {
        tracing::info!("[Remote] Round {} done", round);
    }
    fn on_tool_call(&self, _step_id: &str, tool: &str, args: &str, _status: &str, _stream_id: &str) {
        let preview: String = args.chars().take(120).collect();
        tracing::info!("[Remote] Tool call: {}({})", tool, preview);
    }
    fn on_tool_result(&self, _step_id: &str, tool: &str, result: &str, _status: &str, _stream_id: &str) {
        let preview: String = result.chars().take(300).collect();
        tracing::info!("[Remote] Tool result [{}]: {}", tool, preview);
    }
    fn on_plan_update(&self, plan_text: &str, _stream_id: &str) {
        tracing::info!("[Remote] Plan update:\n{}", plan_text);
    }
    fn on_stream_delta(&self, _delta: &str, _stream_id: &str) {}
    fn on_stream_end(&self, _stream_id: &str) {}
    fn on_final_answer(&self, _content: &str, _stream_id: &str) {}
    fn on_error(&self, error: &str, _stream_id: &str) {
        tracing::error!("[Remote] Error: {}", error);
    }
    fn on_done(&self, _stream_id: &str) {}
    async fn request_approval(&self, _tool: &str, _args: &str, _risk: &str, _stream_id: &str, _step_id: &str) -> bool {
        true
    }
    fn needs_streaming(&self) -> bool { false }
}
