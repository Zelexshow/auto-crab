use crate::models::provider::*;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct TaskStep {
    pub id: usize,
    pub description: String,
    pub status: StepStatus,
    pub result: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StepStatus {
    Pending,
    Running,
    Done,
    Failed,
    Skipped,
}

#[derive(Debug, Clone)]
pub struct TaskPlan {
    pub goal: String,
    pub steps: Vec<TaskStep>,
    pub current_step: usize,
    pub is_complete: bool,
}

impl TaskPlan {
    pub fn current(&self) -> Option<&TaskStep> {
        self.steps.get(self.current_step)
    }

    pub fn advance(&mut self) {
        if self.current_step < self.steps.len() {
            self.current_step += 1;
        }
        if self.current_step >= self.steps.len() {
            self.is_complete = true;
        }
    }

    pub fn mark_current(&mut self, status: StepStatus, result: Option<String>) {
        if let Some(step) = self.steps.get_mut(self.current_step) {
            step.status = status;
            step.result = result;
        }
    }

    pub fn progress_text(&self) -> String {
        let done = self.steps.iter().filter(|s| s.status == StepStatus::Done).count();
        let total = self.steps.len();
        let step_list: Vec<String> = self.steps.iter().enumerate().map(|(i, s)| {
            let marker = match s.status {
                StepStatus::Pending => "⬜",
                StepStatus::Running => "🔄",
                StepStatus::Done => "✅",
                StepStatus::Failed => "❌",
                StepStatus::Skipped => "⏭️",
            };
            let current = if i == self.current_step && !self.is_complete { " ← 当前" } else { "" };
            format!("{} {}. {}{}", marker, i + 1, s.description, current)
        }).collect();
        format!("📋 任务计划 ({}/{})\n目标: {}\n\n{}", done, total, self.goal, step_list.join("\n"))
    }
}

pub struct Planner {
    provider: Arc<dyn ModelProvider>,
}

impl Planner {
    pub fn new(provider: Arc<dyn ModelProvider>) -> Self {
        Self { provider }
    }

    /// Ask the LLM to decompose a complex task into concrete steps.
    pub async fn plan(&self, user_goal: &str) -> anyhow::Result<TaskPlan> {
        let plan_prompt = format!(
r#"你是一个任务规划助手。用户给出一个目标，你需要将其拆分为具体可执行的步骤。

规则：
1. 每个步骤应该是一个原子操作（一次工具调用能完成的）
2. 步骤数量控制在 3-8 步，不要过于细碎
3. 步骤之间有明确的先后依赖
4. 如果任务很简单（1-2步就能完成），就返回少量步骤

用户目标：{user_goal}

请只输出 JSON，格式：
{{"steps": ["步骤1描述", "步骤2描述", "步骤3描述"]}}"#
        );

        let req = ChatRequest {
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: plan_prompt,
                name: None, tool_calls: None, tool_call_id: None,
            }],
            tools: None,
            temperature: 0.3,
            max_tokens: Some(1000),
        };

        let resp = self.provider.chat(req).await?;
        let content = resp.message.content.trim().to_string();

        let steps = parse_plan_response(&content, user_goal)?;

        Ok(TaskPlan {
            goal: user_goal.to_string(),
            steps: steps.into_iter().enumerate().map(|(i, desc)| TaskStep {
                id: i,
                description: desc,
                status: StepStatus::Pending,
                result: None,
            }).collect(),
            current_step: 0,
            is_complete: false,
        })
    }

    /// After executing a step, evaluate the result and decide next action.
    pub async fn reflect(
        &self,
        plan: &TaskPlan,
        step_result: &str,
    ) -> anyhow::Result<ReflectDecision> {
        let current = plan.current().map(|s| s.description.as_str()).unwrap_or("unknown");
        let remaining: Vec<String> = plan.steps.iter()
            .skip(plan.current_step + 1)
            .map(|s| s.description.clone())
            .collect();

        let reflect_prompt = format!(
r#"你是一个任务执行评估器。评估当前步骤的执行结果，决定下一步行动。

任务目标：{}
当前步骤：{}
执行结果：{}
剩余步骤：{:?}

请判断：
1. 当前步骤是否成功完成？
2. 是否需要继续执行剩余步骤？
3. 是否需要调整后续计划？

只输出 JSON：
{{"success": true/false, "action": "continue"/"retry"/"skip"/"abort", "reason": "简要说明", "revised_next_step": "如需修改下一步的描述，写在这里，否则留空"}}"#,
            plan.goal, current, step_result.chars().take(500).collect::<String>(), remaining
        );

        let req = ChatRequest {
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: reflect_prompt,
                name: None, tool_calls: None, tool_call_id: None,
            }],
            tools: None,
            temperature: 0.2,
            max_tokens: Some(500),
        };

        let resp = self.provider.chat(req).await?;
        parse_reflect_response(&resp.message.content)
    }
}

#[derive(Debug)]
pub enum ReflectDecision {
    Continue,
    Retry { reason: String },
    Skip { reason: String },
    Abort { reason: String },
    ReviseAndContinue { revised_step: String },
}

fn parse_plan_response(content: &str, fallback_goal: &str) -> anyhow::Result<Vec<String>> {
    let json_str = extract_json(content);
    let parsed: serde_json::Value = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(_) => {
            return Ok(vec![fallback_goal.to_string()]);
        }
    };

    if let Some(steps) = parsed.get("steps").and_then(|s| s.as_array()) {
        let result: Vec<String> = steps.iter()
            .filter_map(|s| s.as_str().map(String::from))
            .collect();
        if result.is_empty() {
            return Ok(vec![fallback_goal.to_string()]);
        }
        return Ok(result);
    }

    Ok(vec![fallback_goal.to_string()])
}

fn parse_reflect_response(content: &str) -> anyhow::Result<ReflectDecision> {
    let json_str = extract_json(content);
    let parsed: serde_json::Value = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(_) => return Ok(ReflectDecision::Continue),
    };

    let action = parsed.get("action").and_then(|a| a.as_str()).unwrap_or("continue");
    let reason = parsed.get("reason").and_then(|r| r.as_str()).unwrap_or("").to_string();
    let revised = parsed.get("revised_next_step").and_then(|r| r.as_str()).unwrap_or("").to_string();

    match action {
        "retry" => Ok(ReflectDecision::Retry { reason }),
        "skip" => Ok(ReflectDecision::Skip { reason }),
        "abort" => Ok(ReflectDecision::Abort { reason }),
        _ => {
            if !revised.is_empty() {
                Ok(ReflectDecision::ReviseAndContinue { revised_step: revised })
            } else {
                Ok(ReflectDecision::Continue)
            }
        }
    }
}

fn extract_json(text: &str) -> &str {
    let trimmed = text.trim();
    // Handle ```json ... ``` blocks
    if let Some(start) = trimmed.find("```json") {
        let json_start = start + 7;
        if let Some(end) = trimmed[json_start..].find("```") {
            return trimmed[json_start..json_start + end].trim();
        }
    }
    if let Some(start) = trimmed.find("```") {
        let json_start = start + 3;
        // Skip optional language tag on the same line
        let json_start = trimmed[json_start..].find('\n')
            .map(|i| json_start + i + 1)
            .unwrap_or(json_start);
        if let Some(end) = trimmed[json_start..].find("```") {
            return trimmed[json_start..json_start + end].trim();
        }
    }
    // Find raw JSON object
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            return &trimmed[start..=end];
        }
    }
    trimmed
}

/// Heuristic to detect if a user message likely needs multi-step planning.
pub fn should_plan(message: &str) -> bool {
    let msg = message.to_lowercase();
    let len = msg.chars().count();

    if len < 15 { return false; }

    // Single-topic analysis questions should NOT trigger the planner — the model
    // handles them better in one pass with skill prompts. Only truly multi-step
    // *workflows* (do A then B then C) benefit from planning.
    let single_topic_signals = [
        "经济周期", "美林时钟", "资产配置", "投资建议", "行情分析",
        "持仓分析", "宏观分析", "投资简报", "盘分析", "策略建议",
        "周期阶段", "经济形势", "市场分析",
    ];
    if single_topic_signals.iter().any(|kw| msg.contains(*kw)) {
        return false;
    }

    let complex_markers = [
        "并且", "然后", "之后", "同时",
        "帮我做", "完成以下", "步骤",
        "对比", "综合", "系统地",
    ];

    let multi_action = [
        "先.*再.*然后", "先.*再.*最后",
        "搜索.*分析", "截图.*分析", "打开.*输入", "读取.*修改",
        "查看.*总结", "下载.*安装",
    ];

    let marker_count = complex_markers.iter().filter(|kw| msg.contains(*kw)).count();

    if marker_count >= 2 { return true; }

    for pattern in &multi_action {
        let parts: Vec<&str> = pattern.split(".*").collect();
        if parts.len() >= 2 && parts.iter().all(|p| msg.contains(p)) {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_plan_simple_messages() {
        assert!(!should_plan("你好"));
        assert!(!should_plan("今天天气怎么样"));
        assert!(!should_plan("帮我看看桌面有什么文件"));
        assert!(!should_plan("BTC多少钱"));
    }

    #[test]
    fn test_should_plan_complex_messages() {
        assert!(should_plan("帮我搜索一下BTC最新消息，然后分析一下当前走势，给出投资建议"));
        assert!(should_plan("先截图看看当前屏幕，然后分析屏幕上显示了什么内容"));
        assert!(should_plan("帮我读取桌面上的报告文件，分析内容并且生成一份总结报告"));
        assert!(should_plan("查看我的投资组合，综合分析各币种走势，给出完整方案"));
        assert!(should_plan("先打开微信，然后在群里输入今天的市场分析，之后截图确认发送成功"));
    }

    #[test]
    fn test_should_plan_edge_cases() {
        // Short but complex keywords — should NOT trigger (< 15 chars)
        assert!(!should_plan("分析并且总结"));
        // Long enough with one marker
        assert!(should_plan("请帮我详细分析一下这个交易网站上展示的所有数据，包括K线形态和成交量变化趋势"));
    }

    #[test]
    fn test_parse_plan_response_json() {
        let response = r#"{"steps": ["搜索BTC最新新闻", "获取BTC实时价格", "综合分析给出建议"]}"#;
        let steps = parse_plan_response(response, "fallback").unwrap();
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0], "搜索BTC最新新闻");
    }

    #[test]
    fn test_parse_plan_response_markdown_wrapped() {
        let response = "好的，我来分解任务：\n```json\n{\"steps\": [\"步骤1\", \"步骤2\"]}\n```\n以上就是计划。";
        let steps = parse_plan_response(response, "fallback").unwrap();
        assert_eq!(steps.len(), 2);
    }

    #[test]
    fn test_parse_plan_response_invalid_json() {
        let response = "这不是JSON格式的回复";
        let steps = parse_plan_response(response, "原始目标").unwrap();
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0], "原始目标");
    }

    #[test]
    fn test_parse_reflect_response() {
        let response = r#"{"success": true, "action": "continue", "reason": "步骤成功完成"}"#;
        let decision = parse_reflect_response(response).unwrap();
        assert!(matches!(decision, ReflectDecision::Continue));

        let response = r#"{"success": false, "action": "retry", "reason": "网络超时"}"#;
        let decision = parse_reflect_response(response).unwrap();
        assert!(matches!(decision, ReflectDecision::Retry { .. }));

        let response = r#"{"success": false, "action": "abort", "reason": "无法完成"}"#;
        let decision = parse_reflect_response(response).unwrap();
        assert!(matches!(decision, ReflectDecision::Abort { .. }));
    }

    #[test]
    fn test_parse_reflect_response_with_revision() {
        let response = r#"{"success": true, "action": "continue", "reason": "ok", "revised_next_step": "改为直接调用API获取数据"}"#;
        let decision = parse_reflect_response(response).unwrap();
        assert!(matches!(decision, ReflectDecision::ReviseAndContinue { .. }));
    }

    #[test]
    fn test_extract_json() {
        assert_eq!(extract_json(r#"{"a": 1}"#), r#"{"a": 1}"#);
        assert_eq!(extract_json("前面的话 {\"a\": 1} 后面的话"), "{\"a\": 1}");
        assert_eq!(
            extract_json("```json\n{\"steps\": [\"a\"]}\n```"),
            "{\"steps\": [\"a\"]}"
        );
    }

    #[test]
    fn test_task_plan_progress() {
        let mut plan = TaskPlan {
            goal: "测试任务".to_string(),
            steps: vec![
                TaskStep { id: 0, description: "步骤1".into(), status: StepStatus::Done, result: None },
                TaskStep { id: 1, description: "步骤2".into(), status: StepStatus::Running, result: None },
                TaskStep { id: 2, description: "步骤3".into(), status: StepStatus::Pending, result: None },
            ],
            current_step: 1,
            is_complete: false,
        };
        let text = plan.progress_text();
        assert!(text.contains("1/3"));
        assert!(text.contains("测试任务"));
        assert!(text.contains("✅"));
        assert!(text.contains("🔄"));
        assert!(text.contains("⬜"));

        plan.mark_current(StepStatus::Done, Some("result".into()));
        plan.advance();
        assert_eq!(plan.current_step, 2);
        assert!(!plan.is_complete);

        plan.mark_current(StepStatus::Done, None);
        plan.advance();
        assert!(plan.is_complete);
    }
}
