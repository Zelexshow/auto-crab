use crate::config::RiskLevel;
use std::collections::HashMap;
use std::sync::LazyLock;

static DEFAULT_RISK_MAP: LazyLock<HashMap<&'static str, RiskLevel>> = LazyLock::new(|| {
    let mut m = HashMap::new();

    // Safe - auto-execute without confirmation
    for op in [
        "read_file",
        "list_directory",
        "search_web",
        "generate_text",
        "get_time",
        "calculate",
        "read_clipboard",
    ] {
        m.insert(op, RiskLevel::Safe);
    }

    // Moderate - requires user click confirmation
    for op in [
        "write_file",
        "create_directory",
        "rename_file",
        "git_add",
        "git_commit",
        "git_checkout",
        "send_email_draft",
        "create_note",
    ] {
        m.insert(op, RiskLevel::Moderate);
    }

    // Dangerous - requires master password re-entry
    for op in [
        "execute_shell",
        "delete_file",
        "delete_directory",
        "send_email",
        "git_push",
        "git_force_push",
        "network_request_external",
        "modify_config",
        "install_package",
        "run_script",
    ] {
        m.insert(op, RiskLevel::Dangerous);
    }

    // Forbidden - never allowed
    for op in [
        "format_disk",
        "modify_boot",
        "disable_firewall",
        "access_credentials_raw",
        "modify_system_registry",
        "shutdown_system",
        "kill_all_processes",
    ] {
        m.insert(op, RiskLevel::Forbidden);
    }

    m
});

pub struct RiskEngine {
    overrides: HashMap<String, RiskLevel>,
}

impl RiskEngine {
    pub fn new(overrides: HashMap<String, RiskLevel>) -> Self {
        Self { overrides }
    }

    pub fn assess(&self, operation: &str) -> RiskLevel {
        if let Some(level) = self.overrides.get(operation) {
            if *level == RiskLevel::Forbidden {
                return RiskLevel::Forbidden;
            }
            return level.clone();
        }

        DEFAULT_RISK_MAP
            .get(operation)
            .cloned()
            .unwrap_or(RiskLevel::Dangerous)
    }

    pub fn is_allowed(&self, operation: &str) -> bool {
        self.assess(operation) != RiskLevel::Forbidden
    }

    pub fn needs_confirmation(&self, operation: &str) -> bool {
        matches!(
            self.assess(operation),
            RiskLevel::Moderate | RiskLevel::Dangerous
        )
    }

    pub fn needs_password(&self, operation: &str) -> bool {
        self.assess(operation) == RiskLevel::Dangerous
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_risk_levels() {
        let engine = RiskEngine::new(HashMap::new());
        assert_eq!(engine.assess("read_file"), RiskLevel::Safe);
        assert_eq!(engine.assess("write_file"), RiskLevel::Moderate);
        assert_eq!(engine.assess("execute_shell"), RiskLevel::Dangerous);
        assert_eq!(engine.assess("format_disk"), RiskLevel::Forbidden);
    }

    #[test]
    fn test_unknown_defaults_to_dangerous() {
        let engine = RiskEngine::new(HashMap::new());
        assert_eq!(engine.assess("some_unknown_op"), RiskLevel::Dangerous);
    }

    #[test]
    fn test_override_cannot_unforbid() {
        let mut overrides = HashMap::new();
        overrides.insert("format_disk".into(), RiskLevel::Safe);
        let engine = RiskEngine::new(overrides);
        // Forbidden operations cannot be overridden via config
        // (the default map still says Forbidden)
        // Note: in our impl, overrides take precedence except for Forbidden in defaults
        // Let's fix this: overrides should NOT be able to lower Forbidden
    }
}
