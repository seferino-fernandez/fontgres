//! Stdio transport: serves the MCP tools over stdin and stdout.

use anyhow::Context as _;
use rmcp::ServiceExt;

use crate::config::ServerConfig;
use crate::tools::AppTools;

/// Loads the configured fonts, then serves the MCP server over stdio.
pub(crate) async fn serve(config: &ServerConfig) -> anyhow::Result<()> {
    let fonts = config.core.fonts.clone();
    // Scanning system fonts reads many files, so keep it off the async runtime.
    let database = tokio::task::spawn_blocking(move || fonts.build_database())
        .await
        .context("loading fonts")?
        .context("loading fonts")?;
    tracing::info!(faces = database.len(), "fonts loaded");
    let tools = AppTools::new(database, config.mcp.max_response_bytes);
    let service = tools.serve(rmcp::transport::io::stdio_pipes()).await?;
    service.waiting().await?;
    Ok(())
}
