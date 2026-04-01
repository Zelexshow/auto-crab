use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use rmcp::model::{CallToolRequestParams, CallToolResult, RawContent};
use rmcp::service::RunningService;
use rmcp::transport::child_process::TokioChildProcess;
use rmcp::{RoleClient, ServiceExt};

use crate::config::McpServerEntry;
use crate::models::provider::ToolDefinition;

/// A connected MCP server with its discovered tools.
struct ConnectedServer {
    service: RunningService<RoleClient, ()>,
    tools: Vec<ToolDefinition>,
}

/// Manages connections to multiple external MCP servers.
/// Discovers their tools and routes tool calls to the correct server.
pub struct McpClientManager {
    servers: Arc<RwLock<HashMap<String, ConnectedServer>>>,
    /// Maps tool name -> server name for routing.
    tool_routing: Arc<RwLock<HashMap<String, String>>>,
}

impl McpClientManager {
    pub fn new() -> Self {
        Self {
            servers: Arc::new(RwLock::new(HashMap::new())),
            tool_routing: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Connect to all configured MCP servers.
    pub async fn connect_all(&self, configs: &[McpServerEntry]) -> Vec<String> {
        let mut errors = Vec::new();

        for entry in configs {
            if !entry.enabled {
                tracing::info!("[MCP] Skipping disabled server: {}", entry.name);
                continue;
            }
            match self.connect_one(entry).await {
                Ok(count) => {
                    tracing::info!("[MCP] Connected to '{}': {} tools discovered", entry.name, count);
                }
                Err(e) => {
                    let msg = format!("Failed to connect to MCP server '{}': {}", entry.name, e);
                    tracing::error!("[MCP] {}", msg);
                    errors.push(msg);
                }
            }
        }

        errors
    }

    async fn connect_one(&self, entry: &McpServerEntry) -> anyhow::Result<usize> {
        let mut cmd = tokio::process::Command::new(&entry.command);
        cmd.args(&entry.args);
        for (k, v) in &entry.env {
            cmd.env(k, v);
        }

        let proc = TokioChildProcess::new(cmd)
            .map_err(|e| anyhow::anyhow!("Failed to spawn '{}': {}", entry.command, e))?;

        let service = ().serve(proc).await
            .map_err(|e| anyhow::anyhow!("MCP handshake failed for '{}': {}", entry.name, e))?;

        let tools_resp = service.peer().list_tools(Default::default()).await
            .map_err(|e| anyhow::anyhow!("tools/list failed for '{}': {}", entry.name, e))?;

        let prefix = format!("mcp_{}__", entry.name);
        let mut tool_defs = Vec::new();
        let mut routing = self.tool_routing.write().await;

        for tool in &tools_resp.tools {
            let namespaced_name = format!("{}{}", prefix, tool.name);

            let parameters = serde_json::to_value(&tool.input_schema).unwrap_or_default();

            tool_defs.push(ToolDefinition {
                name: namespaced_name.clone(),
                description: format!(
                    "[MCP:{}] {}",
                    entry.name,
                    tool.description.as_deref().unwrap_or(&tool.name)
                ),
                parameters,
            });

            routing.insert(namespaced_name, entry.name.clone());
        }

        let count = tool_defs.len();
        let mut servers = self.servers.write().await;
        servers.insert(entry.name.clone(), ConnectedServer {
            service,
            tools: tool_defs,
        });

        Ok(count)
    }

    /// Get all tool definitions from connected MCP servers.
    pub async fn get_tool_definitions(&self) -> Vec<ToolDefinition> {
        let servers = self.servers.read().await;
        servers.values().flat_map(|s| s.tools.clone()).collect()
    }

    /// Check if a tool name belongs to an MCP server.
    pub async fn is_mcp_tool(&self, tool_name: &str) -> bool {
        let routing = self.tool_routing.read().await;
        routing.contains_key(tool_name)
    }

    /// Execute a tool call on the appropriate MCP server.
    pub async fn call_tool(&self, namespaced_name: &str, arguments: &str) -> anyhow::Result<String> {
        let routing = self.tool_routing.read().await;
        let server_name = routing.get(namespaced_name)
            .ok_or_else(|| anyhow::anyhow!("No MCP server found for tool '{}'", namespaced_name))?
            .clone();
        drop(routing);

        let prefix = format!("mcp_{}__", server_name);
        let original_name = namespaced_name.strip_prefix(&prefix)
            .ok_or_else(|| anyhow::anyhow!("Invalid MCP tool name: {}", namespaced_name))?;

        let args: serde_json::Value = serde_json::from_str(arguments)
            .unwrap_or(serde_json::Value::Object(Default::default()));

        let args_map: serde_json::Map<String, serde_json::Value> = match args {
            serde_json::Value::Object(m) => m,
            _ => Default::default(),
        };

        let servers = self.servers.read().await;
        let server = servers.get(&server_name)
            .ok_or_else(|| anyhow::anyhow!("MCP server '{}' not connected", server_name))?;

        let params = CallToolRequestParams::new(original_name.to_string())
            .with_arguments(args_map);

        let result: CallToolResult = server.service.peer()
            .call_tool(params)
            .await
            .map_err(|e| anyhow::anyhow!("MCP tool call failed: {}", e))?;

        let output: Vec<String> = result.content.iter().map(|c| {
            match &c.raw {
                RawContent::Text(t) => t.text.clone(),
                RawContent::Image(img) => format!("[image: {}]", img.mime_type),
                RawContent::Audio(a) => format!("[audio: {}]", a.mime_type),
                RawContent::Resource(r) => format!("[resource: {:?}]", r.resource),
                RawContent::ResourceLink(rl) => format!("[resource-link: {}]", rl.uri),
            }
        }).collect();

        Ok(output.join("\n"))
    }

    /// Disconnect all servers gracefully.
    pub async fn disconnect_all(&self) {
        let mut servers = self.servers.write().await;
        for (name, server) in servers.drain() {
            tracing::info!("[MCP] Disconnecting from '{}'", name);
            drop(server.service);
        }
        self.tool_routing.write().await.clear();
    }

    /// Get summary of connected servers and their tool counts.
    pub async fn status(&self) -> Vec<(String, usize)> {
        let servers = self.servers.read().await;
        servers.iter().map(|(name, s)| (name.clone(), s.tools.len())).collect()
    }
}
