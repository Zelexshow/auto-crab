use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub operation_type: String,
    pub parameters: Vec<ToolParam>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolParam {
    pub name: String,
    pub param_type: String,
    pub description: String,
    pub required: bool,
}

pub struct ToolRegistry {
    tools: HashMap<String, ToolSpec>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        let mut registry = Self { tools: HashMap::new() };
        registry.register_builtins();
        registry
    }

    fn register_builtins(&mut self) {
        self.register(ToolSpec {
            name: "read_file".into(),
            description: "Read the contents of a file".into(),
            operation_type: "read_file".into(),
            parameters: vec![ToolParam {
                name: "path".into(),
                param_type: "string".into(),
                description: "Path to the file to read".into(),
                required: true,
            }],
        });

        self.register(ToolSpec {
            name: "write_file".into(),
            description: "Write content to a file".into(),
            operation_type: "write_file".into(),
            parameters: vec![
                ToolParam {
                    name: "path".into(),
                    param_type: "string".into(),
                    description: "Path to the file to write".into(),
                    required: true,
                },
                ToolParam {
                    name: "content".into(),
                    param_type: "string".into(),
                    description: "Content to write".into(),
                    required: true,
                },
            ],
        });

        self.register(ToolSpec {
            name: "list_directory".into(),
            description: "List files and directories in a path".into(),
            operation_type: "list_directory".into(),
            parameters: vec![ToolParam {
                name: "path".into(),
                param_type: "string".into(),
                description: "Directory path to list".into(),
                required: true,
            }],
        });

        self.register(ToolSpec {
            name: "execute_shell".into(),
            description: "Execute a shell command".into(),
            operation_type: "execute_shell".into(),
            parameters: vec![
                ToolParam {
                    name: "command".into(),
                    param_type: "string".into(),
                    description: "The command to execute".into(),
                    required: true,
                },
                ToolParam {
                    name: "working_directory".into(),
                    param_type: "string".into(),
                    description: "Working directory for the command".into(),
                    required: false,
                },
            ],
        });

        self.register(ToolSpec {
            name: "search_web".into(),
            description: "Search the web for information".into(),
            operation_type: "search_web".into(),
            parameters: vec![ToolParam {
                name: "query".into(),
                param_type: "string".into(),
                description: "Search query".into(),
                required: true,
            }],
        });
    }

    pub fn register(&mut self, spec: ToolSpec) {
        self.tools.insert(spec.name.clone(), spec);
    }

    pub fn get(&self, name: &str) -> Option<&ToolSpec> {
        self.tools.get(name)
    }

    pub fn list(&self) -> Vec<&ToolSpec> {
        self.tools.values().collect()
    }

    pub fn to_tool_definitions(&self) -> Vec<crate::models::provider::ToolDefinition> {
        self.tools.values().map(|spec| {
            let properties: serde_json::Map<String, serde_json::Value> = spec.parameters.iter().map(|p| {
                (p.name.clone(), serde_json::json!({
                    "type": p.param_type,
                    "description": p.description,
                }))
            }).collect();
            let required: Vec<&str> = spec.parameters.iter()
                .filter(|p| p.required)
                .map(|p| p.name.as_str())
                .collect();

            crate::models::provider::ToolDefinition {
                name: spec.name.clone(),
                description: spec.description.clone(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": properties,
                    "required": required,
                }),
            }
        }).collect()
    }

    pub fn to_openai_tools(&self) -> Vec<serde_json::Value> {
        self.tools.values().map(|spec| {
            let properties: serde_json::Map<String, serde_json::Value> = spec.parameters.iter().map(|p| {
                (p.name.clone(), serde_json::json!({
                    "type": p.param_type,
                    "description": p.description,
                }))
            }).collect();
            let required: Vec<&str> = spec.parameters.iter()
                .filter(|p| p.required)
                .map(|p| p.name.as_str())
                .collect();

            serde_json::json!({
                "type": "function",
                "function": {
                    "name": spec.name,
                    "description": spec.description,
                    "parameters": {
                        "type": "object",
                        "properties": properties,
                        "required": required,
                    }
                }
            })
        }).collect()
    }
}
