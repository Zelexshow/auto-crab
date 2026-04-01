use rmcp::handler::server::tool::ToolRouter;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::model::{Implementation, ServerCapabilities, ServerInfo};
use rmcp::{tool, tool_router, ErrorData as McpError, ServerHandler, ServiceExt};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, JsonSchema, Default)]
pub struct SearchWebParams {
    /// Search query string
    pub query: String,
}

#[derive(Serialize, JsonSchema)]
pub struct SearchWebResult {
    pub results: String,
}

#[derive(Deserialize, JsonSchema, Default)]
pub struct MarketPriceParams {
    /// Asset query - stock name/code, commodity, crypto. Examples: '茅台', 'AAPL', '黄金', 'BTCUSDT'
    pub query: String,
}

#[derive(Serialize, JsonSchema)]
pub struct MarketPriceResult {
    pub data: String,
}

#[derive(Deserialize, JsonSchema, Default)]
pub struct ReadFileParams {
    /// Path to the file to read
    pub path: String,
}

#[derive(Serialize, JsonSchema)]
pub struct ReadFileResult {
    pub content: String,
}

#[derive(Deserialize, JsonSchema, Default)]
pub struct FetchWebpageParams {
    /// URL to fetch
    pub url: String,
}

#[derive(Serialize, JsonSchema)]
pub struct FetchWebpageResult {
    pub content: String,
}

#[derive(Deserialize, JsonSchema, Default)]
pub struct ShellParams {
    /// The command to execute
    pub command: String,
}

#[derive(Serialize, JsonSchema)]
pub struct ShellResult {
    pub output: String,
}

/// MCP Server that exposes Auto-Crab's builtin tools to external AI clients.
pub struct AutoCrabMcpServer {
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

impl AutoCrabMcpServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl AutoCrabMcpServer {
    #[tool(
        name = "search_web",
        description = "Search the web for information. Returns titles, snippets, and URLs from multiple search engines."
    )]
    async fn search_web(
        &self,
        Parameters(params): Parameters<SearchWebParams>,
    ) -> Result<Json<SearchWebResult>, McpError> {
        match crate::commands::search_web_pub(&params.query).await {
            Ok(results) => Ok(Json(SearchWebResult { results })),
            Err(e) => Err(McpError::internal_error(
                format!("Search failed: {}", e),
                None,
            )),
        }
    }

    #[tool(
        name = "get_market_price",
        description = "Get real-time market price for financial assets: stocks (A-share, HK, US, JP), gold, silver, oil, crypto, forex. Accepts natural language like '茅台', 'AAPL', '黄金', 'BTCUSDT'."
    )]
    async fn get_market_price(
        &self,
        Parameters(params): Parameters<MarketPriceParams>,
    ) -> Result<Json<MarketPriceResult>, McpError> {
        match crate::commands::fetch_market_price_pub(&params.query).await {
            Ok(data) => Ok(Json(MarketPriceResult { data })),
            Err(e) => Err(McpError::internal_error(
                format!("Market data fetch failed: {}", e),
                None,
            )),
        }
    }

    #[tool(
        name = "read_file",
        description = "Read the contents of a file from the local filesystem."
    )]
    async fn read_file(
        &self,
        Parameters(params): Parameters<ReadFileParams>,
    ) -> Result<Json<ReadFileResult>, McpError> {
        match tokio::fs::read_to_string(&params.path).await {
            Ok(content) => {
                let truncated = if content.len() > 50_000 {
                    format!(
                        "{}...\n[truncated, {} total chars]",
                        &content[..50_000],
                        content.len()
                    )
                } else {
                    content
                };
                Ok(Json(ReadFileResult { content: truncated }))
            }
            Err(e) => Err(McpError::internal_error(
                format!("Read failed: {}", e),
                None,
            )),
        }
    }

    #[tool(
        name = "fetch_webpage",
        description = "Fetch the text content of a webpage URL. Returns page text without HTML tags."
    )]
    async fn fetch_webpage(
        &self,
        Parameters(params): Parameters<FetchWebpageParams>,
    ) -> Result<Json<FetchWebpageResult>, McpError> {
        match crate::commands::fetch_webpage_pub(&params.url).await {
            Ok(content) => Ok(Json(FetchWebpageResult { content })),
            Err(e) => Err(McpError::internal_error(
                format!("Fetch failed: {}", e),
                None,
            )),
        }
    }

    #[tool(
        name = "execute_shell",
        description = "Execute a shell command and return its output."
    )]
    async fn execute_shell(
        &self,
        Parameters(params): Parameters<ShellParams>,
    ) -> Result<Json<ShellResult>, McpError> {
        let shell = if cfg!(target_os = "windows") {
            "cmd"
        } else {
            "sh"
        };
        let flag = if cfg!(target_os = "windows") {
            "/C"
        } else {
            "-c"
        };

        let output = tokio::process::Command::new(shell)
            .arg(flag)
            .arg(&params.command)
            .output()
            .await
            .map_err(|e| {
                McpError::internal_error(format!("Shell exec failed: {}", e), None)
            })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let result = if stderr.is_empty() {
            stdout.to_string()
        } else {
            format!("{}\n[stderr] {}", stdout, stderr)
        };

        Ok(Json(ShellResult { output: result }))
    }
}

impl ServerHandler for AutoCrabMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .build(),
        )
        .with_server_info(Implementation::new("auto-crab", env!("CARGO_PKG_VERSION")))
        .with_instructions(
            "Auto-Crab AI Desktop Assistant. Provides web search, real-time market data, \
             file operations, web scraping, and shell execution.",
        )
    }
}

/// Start the MCP server on stdio (blocking, for use as a subprocess).
pub async fn run_stdio_server() -> anyhow::Result<()> {
    tracing::info!("[MCP Server] Starting on stdio...");

    let server = AutoCrabMcpServer::new();
    let service = server
        .serve((tokio::io::stdin(), tokio::io::stdout()))
        .await
        .map_err(|e| anyhow::anyhow!("MCP server failed to start: {}", e))?;

    tracing::info!("[MCP Server] Running, waiting for client...");
    service
        .waiting()
        .await
        .map_err(|e| anyhow::anyhow!("MCP server error: {}", e))?;

    tracing::info!("[MCP Server] Shutting down");
    Ok(())
}
