//! Stdio transport: serves the MCP tools over stdin and stdout.

use rmcp::ServiceExt;

use crate::config::ServerConfig;
use crate::tools::AppTools;

/// Serves the MCP server over stdio.
pub(crate) async fn serve(config: &ServerConfig) -> anyhow::Result<()> {
    let tools = AppTools::new(config.mcp.max_response_bytes);
    let service = tools.serve(rmcp::transport::io::stdio_pipes()).await?;
    service.waiting().await?;
    Ok(())
}
