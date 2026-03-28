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
        let mut registry = Self {
            tools: HashMap::new(),
        };
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
            description: "Search the web for information using Bing (China-accessible) with DuckDuckGo fallback. Returns titles, snippets, and URLs.".into(),
            operation_type: "search_web".into(),
            parameters: vec![ToolParam {
                name: "query".into(),
                param_type: "string".into(),
                description: "Search query".into(),
                required: true,
            }],
        });

        self.register(ToolSpec {
            name: "screenshot".into(),
            description: "Take a screenshot of the primary screen and save it to a file. Returns the file path of the saved screenshot.".into(),
            operation_type: "read_file".into(),
            parameters: vec![ToolParam {
                name: "output_path".into(),
                param_type: "string".into(),
                description: "File path to save the screenshot PNG (e.g. C:\\Users\\zelex\\Desktop\\screenshot.png)".into(),
                required: false,
            }],
        });

        self.register(ToolSpec {
            name: "mouse_click".into(),
            description: "Move the mouse to a specific screen coordinate and click. Use after analyze_screen to interact with UI elements.".into(),
            operation_type: "execute_shell".into(),
            parameters: vec![
                ToolParam { name: "x".into(), param_type: "integer".into(), description: "X coordinate (pixels from left)".into(), required: true },
                ToolParam { name: "y".into(), param_type: "integer".into(), description: "Y coordinate (pixels from top)".into(), required: true },
                ToolParam { name: "click_type".into(), param_type: "string".into(), description: "Click type: left (default), right, double".into(), required: false },
            ],
        });

        self.register(ToolSpec {
            name: "keyboard_type".into(),
            description:
                "Type text using the keyboard. Use after clicking on a text field to input text."
                    .into(),
            operation_type: "execute_shell".into(),
            parameters: vec![ToolParam {
                name: "text".into(),
                param_type: "string".into(),
                description: "Text to type".into(),
                required: true,
            }],
        });

        self.register(ToolSpec {
            name: "key_press".into(),
            description: "Press a special key or key combination. Examples: enter, tab, escape, ctrl+a, ctrl+c, ctrl+v, alt+tab, backspace, delete.".into(),
            operation_type: "execute_shell".into(),
            parameters: vec![
                ToolParam { name: "key".into(), param_type: "string".into(), description: "Key name or combo (e.g. enter, ctrl+a, alt+tab)".into(), required: true },
            ],
        });

        self.register(ToolSpec {
            name: "fetch_webpage".into(),
            description: "Fetch the text content of a webpage URL. Use this for reading articles, documentation, data pages. Returns the page text without HTML tags.".into(),
            operation_type: "read_file".into(),
            parameters: vec![ToolParam {
                name: "url".into(),
                param_type: "string".into(),
                description: "The URL to fetch (e.g. https://example.com/article)".into(),
                required: true,
            }],
        });

        self.register(ToolSpec {
            name: "read_pdf".into(),
            description: "Read and extract text content from a PDF file. Returns the text for summarization or analysis.".into(),
            operation_type: "read_file".into(),
            parameters: vec![
                ToolParam { name: "path".into(), param_type: "string".into(), description: "Path to the PDF file".into(), required: true },
                ToolParam { name: "max_pages".into(), param_type: "integer".into(), description: "Max pages to read (default: 20)".into(), required: false },
            ],
        });

        self.register(ToolSpec {
            name: "get_crypto_price".into(),
            description: "Get real-time cryptocurrency price from Binance API. Returns current price, 24h change, high, low, volume. No screenshot needed - direct API data.".into(),
            operation_type: "read_file".into(),
            parameters: vec![
                ToolParam { name: "symbol".into(), param_type: "string".into(), description: "Trading pair symbol, e.g. BTCUSDT, ETHUSDT, SOLUSDT".into(), required: true },
            ],
        });

        self.register(ToolSpec {
            name: "quick_reply_wechat".into(),
            description: "Send a message to a WeChat contact quickly. Focuses WeChat window, finds the contact in chat list, clicks input box, types message, and sends. This is a high-level macro - use this instead of manual analyze_screen + mouse_click sequences for WeChat replies.".into(),
            operation_type: "execute_shell".into(),
            parameters: vec![
                ToolParam { name: "contact".into(), param_type: "string".into(), description: "Contact name or group name to send to".into(), required: true },
                ToolParam { name: "message".into(), param_type: "string".into(), description: "Message text to send".into(), required: true },
            ],
        });

        self.register(ToolSpec {
            name: "get_ui_tree".into(),
            description: "Get the UI element tree of the foreground window. Returns structured text of all controls with types, names, coordinates, and states. 10x FASTER than analyze_screen and gives EXACT coordinates. Use this FIRST for standard apps (WeChat, browsers, etc). Falls back to analyze_screen only for Canvas/game UIs.".into(),
            operation_type: "read_file".into(),
            parameters: vec![
                ToolParam { name: "window_title".into(), param_type: "string".into(), description: "Optional: specific window title (default: foreground window)".into(), required: false },
                ToolParam { name: "max_depth".into(), param_type: "integer".into(), description: "Max tree depth (default: 8)".into(), required: false },
            ],
        });

        self.register(ToolSpec {
            name: "focus_window".into(),
            description: "Bring a window to the foreground by its title (partial match). Use this BEFORE interacting with a specific app to ensure it's focused.".into(),
            operation_type: "execute_shell".into(),
            parameters: vec![ToolParam { name: "title".into(), param_type: "string".into(), description: "Window title to search for (partial match, e.g. '微信')".into(), required: true }],
        });

        self.register(ToolSpec {
            name: "analyze_and_act".into(),
            description: "Take a screenshot, analyze it with AI vision, and execute actions (click/type/key_press) in ONE step. This is the PREFERRED tool for desktop automation - it combines seeing and acting. Much faster than calling analyze_screen + mouse_click separately. Returns result after executing.".into(),
            operation_type: "execute_shell".into(),
            parameters: vec![
                ToolParam { name: "task".into(), param_type: "string".into(), description: "The task to accomplish (e.g. 'click the Send button', 'type hello in the input box', 'find and click WeChat in the chat list')".into(), required: true },
                ToolParam { name: "max_steps".into(), param_type: "integer".into(), description: "Maximum action steps (default: 3)".into(), required: false },
            ],
        });

        self.register(ToolSpec {
            name: "analyze_screen".into(),
            description: "Take a screenshot and analyze it with AI vision. Returns text description AND pixel coordinates of UI elements. Use this BEFORE mouse_click to find where to click. The coordinates returned can be directly used with mouse_click(x, y).".into(),
            operation_type: "read_file".into(),
            parameters: vec![ToolParam {
                name: "question".into(),
                param_type: "string".into(),
                description: "What to focus on when analyzing the screenshot (e.g. 'what messages are visible in WeChat')".into(),
                required: false,
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
        self.tools
            .values()
            .map(|spec| {
                let properties: serde_json::Map<String, serde_json::Value> = spec
                    .parameters
                    .iter()
                    .map(|p| {
                        (
                            p.name.clone(),
                            serde_json::json!({
                                "type": p.param_type,
                                "description": p.description,
                            }),
                        )
                    })
                    .collect();
                let required: Vec<&str> = spec
                    .parameters
                    .iter()
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
            })
            .collect()
    }

    pub fn to_openai_tools(&self) -> Vec<serde_json::Value> {
        self.tools
            .values()
            .map(|spec| {
                let properties: serde_json::Map<String, serde_json::Value> = spec
                    .parameters
                    .iter()
                    .map(|p| {
                        (
                            p.name.clone(),
                            serde_json::json!({
                                "type": p.param_type,
                                "description": p.description,
                            }),
                        )
                    })
                    .collect();
                let required: Vec<&str> = spec
                    .parameters
                    .iter()
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
            })
            .collect()
    }
}
