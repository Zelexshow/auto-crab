use std::collections::HashSet;
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq)]
pub enum HookEvent {
    PreToolUse,
    PostToolUse,
    Stop,
}

#[derive(Debug, Clone)]
pub enum HookDecision {
    Allow,
    Deny { reason: String },
    Warn { message: String },
}

#[derive(Debug, Clone)]
pub struct SecurityPattern {
    pub name: String,
    pub substrings: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct PathPattern {
    pub name: String,
    pub path_contains: Vec<String>,
    pub path_suffix: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct ShellDangerPattern {
    pub name: String,
    pub patterns: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HookRuleConfig {
    pub name: String,
    pub event: String,
    pub tool_matcher: String,
    pub content_substrings: Vec<String>,
    #[serde(default)]
    pub path_patterns: Vec<String>,
    pub action: String,
    pub message: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

pub struct HookManager {
    security_patterns: Vec<SecurityPattern>,
    path_patterns: Vec<PathPattern>,
    shell_patterns: Vec<ShellDangerPattern>,
    custom_rules: Vec<HookRuleConfig>,
    shown_warnings: Mutex<HashSet<String>>,
    security_enabled: bool,
    stop_verification: bool,
}

impl HookManager {
    pub fn new(security_enabled: bool, stop_verification: bool, custom_rules: Vec<HookRuleConfig>) -> Self {
        Self {
            security_patterns: builtin_security_patterns(),
            path_patterns: builtin_path_patterns(),
            shell_patterns: builtin_shell_patterns(),
            custom_rules,
            shown_warnings: Mutex::new(HashSet::new()),
            security_enabled,
            stop_verification,
        }
    }

    pub fn run_pre_tool_use(&self, tool_name: &str, tool_args: &str) -> HookDecision {
        if !self.security_enabled {
            return HookDecision::Allow;
        }

        // Check custom deny rules first
        for rule in &self.custom_rules {
            if !rule.enabled || rule.event != "pre_tool_use" {
                continue;
            }
            if !matches_tool(&rule.tool_matcher, tool_name) {
                continue;
            }
            if rule.content_substrings.iter().any(|s| tool_args.contains(s.as_str())) {
                let decision = match rule.action.as_str() {
                    "deny" => HookDecision::Deny { reason: rule.message.clone() },
                    _ => HookDecision::Warn { message: rule.message.clone() },
                };
                return decision;
            }
        }

        // Shell command safety checks
        if tool_name == "execute_shell" {
            if let Some(decision) = self.check_shell_patterns(tool_args) {
                return decision;
            }
        }

        // File write content safety checks
        if matches!(tool_name, "write_file" | "execute_shell") {
            if let Some(decision) = self.check_content_patterns(tool_name, tool_args) {
                return decision;
            }
        }

        // Path-based checks for file operations
        if matches!(tool_name, "write_file" | "read_file" | "delete_file") {
            if let Some(decision) = self.check_path_patterns(tool_args) {
                return decision;
            }
        }

        HookDecision::Allow
    }

    pub fn run_post_tool_use(&self, tool_name: &str, _tool_args: &str, _result: &str) -> Option<String> {
        // Post-tool hooks: run custom rules that want feedback injection
        for rule in &self.custom_rules {
            if !rule.enabled || rule.event != "post_tool_use" {
                continue;
            }
            if matches_tool(&rule.tool_matcher, tool_name) {
                return Some(rule.message.clone());
            }
        }
        None
    }

    pub fn run_stop_check(&self, has_tool_calls: bool, rounds_completed: usize) -> HookDecision {
        if !self.stop_verification {
            return HookDecision::Allow;
        }

        // If the agent used tools but completed very few rounds, it may be stopping too early
        if has_tool_calls && rounds_completed <= 1 {
            return HookDecision::Warn {
                message: "Agent completed very quickly after tool use. Consider verifying task completeness.".to_string(),
            };
        }

        // Run custom stop rules
        for rule in &self.custom_rules {
            if !rule.enabled || rule.event != "stop" {
                continue;
            }
            match rule.action.as_str() {
                "deny" => return HookDecision::Deny { reason: rule.message.clone() },
                "warn" => return HookDecision::Warn { message: rule.message.clone() },
                _ => {}
            }
        }

        HookDecision::Allow
    }

    fn check_shell_patterns(&self, args: &str) -> Option<HookDecision> {
        let args_lower = args.to_lowercase();
        for pat in &self.shell_patterns {
            for p in &pat.patterns {
                if args_lower.contains(p.as_str()) {
                    let key = format!("shell-{}", pat.name);
                    return Some(self.deduped_warn(&key, &pat.message));
                }
            }
        }
        None
    }

    fn check_content_patterns(&self, tool_name: &str, args: &str) -> Option<HookDecision> {
        for pat in &self.security_patterns {
            for sub in &pat.substrings {
                if args.contains(sub.as_str()) {
                    let key = format!("{}-{}", tool_name, pat.name);
                    return Some(self.deduped_warn(&key, &pat.message));
                }
            }
        }
        None
    }

    fn check_path_patterns(&self, args: &str) -> Option<HookDecision> {
        let args_lower = args.to_lowercase().replace('\\', "/");

        // Extract the file path from JSON args (look for "path" or "file_path" fields)
        let file_path = extract_path_from_args(&args_lower);
        let check_target = file_path.as_deref().unwrap_or(&args_lower);

        for pat in &self.path_patterns {
            let matched = pat.path_contains.iter().any(|c| check_target.contains(c.as_str()))
                || pat.path_suffix.iter().any(|s| check_target.ends_with(s.as_str()));
            if matched {
                let key = format!("path-{}", pat.name);
                return Some(self.deduped_warn(&key, &pat.message));
            }
        }
        None
    }

    /// Return a Warn decision, but only once per session for a given key.
    /// Subsequent hits for the same key return Allow to avoid nagging.
    fn deduped_warn(&self, key: &str, message: &str) -> HookDecision {
        let mut shown = self.shown_warnings.lock().unwrap_or_else(|e| e.into_inner());
        if shown.contains(key) {
            return HookDecision::Allow;
        }
        shown.insert(key.to_string());
        HookDecision::Warn { message: message.to_string() }
    }
}

fn extract_path_from_args(args: &str) -> Option<String> {
    // Simple JSON path extraction without pulling in a full parser.
    // Looks for "path": "..." or "file_path": "..." patterns.
    for key in &["\"path\"", "\"file_path\"", "\"file\""] {
        if let Some(key_pos) = args.find(key) {
            let after_key = &args[key_pos + key.len()..];
            // Skip colon and whitespace
            let after_colon = after_key.trim_start().strip_prefix(':')?;
            let after_ws = after_colon.trim_start().strip_prefix('"')?;
            if let Some(end_quote) = after_ws.find('"') {
                return Some(after_ws[..end_quote].to_string());
            }
        }
    }
    None
}

fn matches_tool(matcher: &str, tool_name: &str) -> bool {
    if matcher == "*" {
        return true;
    }
    for part in matcher.split('|') {
        let part = part.trim();
        if part == tool_name {
            return true;
        }
        if part.ends_with('*') && tool_name.starts_with(&part[..part.len() - 1]) {
            return true;
        }
    }
    false
}

fn builtin_security_patterns() -> Vec<SecurityPattern> {
    vec![
        SecurityPattern {
            name: "eval_injection".into(),
            substrings: vec!["eval(".into()],
            message: "Security: eval() executes arbitrary code and is a major security risk. \
                      Consider using JSON.parse() for data parsing or safer alternatives.".into(),
        },
        SecurityPattern {
            name: "new_function_injection".into(),
            substrings: vec!["new Function".into()],
            message: "Security: new Function() with dynamic strings can lead to code injection. \
                      Consider alternative approaches that don't evaluate arbitrary code.".into(),
        },
        SecurityPattern {
            name: "child_process_exec".into(),
            substrings: vec!["child_process.exec".into(), "execSync(".into()],
            message: "Security: child_process.exec() can lead to command injection. \
                      Use execFile() or spawn() with argument arrays instead.".into(),
        },
        SecurityPattern {
            name: "innerHTML_xss".into(),
            substrings: vec![".innerHTML =".into(), ".innerHTML=".into()],
            message: "Security: Setting innerHTML with untrusted content can lead to XSS. \
                      Use textContent for plain text or sanitize HTML with DOMPurify.".into(),
        },
        SecurityPattern {
            name: "dangerously_set_html".into(),
            substrings: vec!["dangerouslySetInnerHTML".into()],
            message: "Security: dangerouslySetInnerHTML can lead to XSS if used with untrusted content. \
                      Ensure all content is properly sanitized.".into(),
        },
        SecurityPattern {
            name: "document_write".into(),
            substrings: vec!["document.write".into()],
            message: "Security: document.write() can be exploited for XSS. \
                      Use DOM manipulation methods like createElement() instead.".into(),
        },
        SecurityPattern {
            name: "pickle_deserialization".into(),
            substrings: vec!["pickle.load".into(), "pickle.loads".into()],
            message: "Security: pickle with untrusted content can lead to arbitrary code execution. \
                      Consider using JSON or other safe serialization formats.".into(),
        },
        SecurityPattern {
            name: "os_system".into(),
            substrings: vec!["os.system(".into(), "from os import system".into()],
            message: "Security: os.system() is vulnerable to shell injection. \
                      Use subprocess.run() with shell=False and argument lists instead.".into(),
        },
        SecurityPattern {
            name: "sql_injection".into(),
            substrings: vec!["f\"SELECT".into(), "f'SELECT".into(), "format!(\"SELECT".into()],
            message: "Security: String-interpolated SQL is vulnerable to injection. \
                      Use parameterized queries or an ORM instead.".into(),
        },
    ]
}

fn builtin_path_patterns() -> Vec<PathPattern> {
    vec![
        PathPattern {
            name: "env_file".into(),
            path_contains: vec![],
            path_suffix: vec![".env".into(), ".env.local".into(), ".env.production".into()],
            message: "Security: .env files often contain secrets (API keys, passwords). \
                      Verify that no credentials are being written.".into(),
        },
        PathPattern {
            name: "github_workflows".into(),
            path_contains: vec![".github/workflows/".into()],
            path_suffix: vec![],
            message: "Security: GitHub Actions workflow files can execute arbitrary code. \
                      Never use untrusted input directly in run: commands.".into(),
        },
        PathPattern {
            name: "ssh_keys".into(),
            path_contains: vec![".ssh/".into()],
            path_suffix: vec![],
            message: "Security: SSH key directory detected. Be extremely careful with key material.".into(),
        },
        PathPattern {
            name: "credentials".into(),
            path_contains: vec![],
            path_suffix: vec![
                "credentials.json".into(),
                "credentials.yaml".into(),
                "secrets.json".into(),
                "secrets.yaml".into(),
            ],
            message: "Security: Credentials file detected. Verify no secrets are being exposed.".into(),
        },
    ]
}

fn builtin_shell_patterns() -> Vec<ShellDangerPattern> {
    vec![
        ShellDangerPattern {
            name: "destructive_rm".into(),
            patterns: vec!["rm -rf /".into(), "rm -rf /*".into(), "del /s /q c:\\".into()],
            message: "DANGER: Destructive file deletion command detected. This could erase critical data.".into(),
        },
        ShellDangerPattern {
            name: "chmod_world_writable".into(),
            patterns: vec!["chmod 777".into(), "chmod -r 777".into()],
            message: "Security: chmod 777 makes files world-writable. Use more restrictive permissions.".into(),
        },
        ShellDangerPattern {
            name: "curl_pipe_sh".into(),
            patterns: vec!["| sh".into(), "| bash".into(), "| sudo".into()],
            message: "Security: Piping download output directly to a shell is dangerous. \
                      Download the script first, review it, then execute.".into(),
        },
        ShellDangerPattern {
            name: "format_disk".into(),
            patterns: vec!["format c:".into(), "mkfs".into(), "dd if=".into()],
            message: "DANGER: Disk formatting command detected. This will destroy all data.".into(),
        },
        ShellDangerPattern {
            name: "disable_firewall".into(),
            patterns: vec![
                "netsh advfirewall set allprofiles state off".into(),
                "ufw disable".into(),
                "iptables -f".into(),
            ],
            message: "DANGER: Firewall disable command detected. This weakens system security.".into(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pre_tool_use_eval_detection() {
        let hm = HookManager::new(true, false, vec![]);
        let decision = hm.run_pre_tool_use("write_file", r#"{"path": "test.js", "content": "eval(userInput)"}"#);
        assert!(matches!(decision, HookDecision::Warn { .. }));
    }

    #[test]
    fn test_pre_tool_use_safe_content() {
        let hm = HookManager::new(true, false, vec![]);
        let decision = hm.run_pre_tool_use("write_file", r#"{"path": "test.rs", "content": "fn main() {}"}"#);
        assert!(matches!(decision, HookDecision::Allow));
    }

    #[test]
    fn test_shell_danger_detection() {
        let hm = HookManager::new(true, false, vec![]);
        let decision = hm.run_pre_tool_use("execute_shell", r#"{"command": "rm -rf /"}"#);
        assert!(matches!(decision, HookDecision::Warn { .. }));
    }

    #[test]
    fn test_session_dedup() {
        let hm = HookManager::new(true, false, vec![]);
        let d1 = hm.run_pre_tool_use("write_file", r#"{"content": "eval(x)"}"#);
        assert!(matches!(d1, HookDecision::Warn { .. }));
        // Same pattern again should be allowed (deduplicated)
        let d2 = hm.run_pre_tool_use("write_file", r#"{"content": "eval(y)"}"#);
        assert!(matches!(d2, HookDecision::Allow));
    }

    #[test]
    fn test_disabled_security() {
        let hm = HookManager::new(false, false, vec![]);
        let decision = hm.run_pre_tool_use("write_file", r#"{"content": "eval(x)"}"#);
        assert!(matches!(decision, HookDecision::Allow));
    }

    #[test]
    fn test_custom_deny_rule() {
        let rules = vec![HookRuleConfig {
            name: "block-npm-install".into(),
            event: "pre_tool_use".into(),
            tool_matcher: "execute_shell".into(),
            content_substrings: vec!["npm install".into()],
            path_patterns: vec![],
            action: "deny".into(),
            message: "npm install is blocked by policy".into(),
            enabled: true,
        }];
        let hm = HookManager::new(true, false, rules);
        let decision = hm.run_pre_tool_use("execute_shell", r#"{"command": "npm install express"}"#);
        assert!(matches!(decision, HookDecision::Deny { .. }));
    }

    #[test]
    fn test_path_env_detection() {
        let hm = HookManager::new(true, false, vec![]);
        let decision = hm.run_pre_tool_use("write_file", r#"{"path": "/project/.env", "content": "KEY=value"}"#);
        assert!(matches!(decision, HookDecision::Warn { .. }));
    }

    #[test]
    fn test_tool_matcher() {
        assert!(matches_tool("*", "write_file"));
        assert!(matches_tool("write_file", "write_file"));
        assert!(!matches_tool("write_file", "read_file"));
        assert!(matches_tool("execute_shell|write_file", "write_file"));
        assert!(matches_tool("mcp_*", "mcp_github_tool"));
        assert!(!matches_tool("mcp_*", "write_file"));
    }

    #[test]
    fn test_stop_check() {
        let hm = HookManager::new(true, true, vec![]);
        let d = hm.run_stop_check(true, 1);
        assert!(matches!(d, HookDecision::Warn { .. }));
        let d2 = hm.run_stop_check(true, 5);
        assert!(matches!(d2, HookDecision::Allow));
    }
}
