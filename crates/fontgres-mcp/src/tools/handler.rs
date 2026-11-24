//! [`AppTools`]: the rmcp tool handler. Owns the font database and routes tool
//! calls through the combined tool router, built once per process.

use std::ops::Deref;
use std::sync::Arc;

use rmcp::handler::server::tool::ToolRouter;
use rmcp::model::Tool;

use fontgres_core::Database;

use super::server::close_tool_schemas;

/// MCP tool handler for the font database.
///
/// A cheap handle: cloning it bumps one reference count, so every request
/// shares the font database, the tool router and the advertised tools built by
/// [`AppTools::new`].
#[derive(Clone)]
pub(crate) struct AppTools(Arc<Inner>);

/// The state every [`AppTools`] handle shares.
#[derive(Clone)]
pub(crate) struct Inner {
    /// The fonts every tool reads, loaded once at startup.
    pub(super) database: Arc<Database>,
    /// Every tool route, with its input and output schemas already closed.
    pub(super) router: ToolRouter<AppTools>,
    /// The tools `tools/list` advertises: the router's tools, sorted by name.
    /// They share their schemas with the router.
    pub(super) advertised: Vec<Tool>,
    /// Maximum exact serialized size (bytes) of a tool result. The list tools
    /// cut each page to it; any other result over it is replaced with a
    /// bounded `response_too_large` result or, when the limit cannot hold
    /// that diagnostic, a protocol error.
    pub(super) max_response_bytes: usize,
}

impl AppTools {
    /// Builds a handler around an already loaded font database.
    ///
    /// Combines every tool router and closes each tool's schemas once, so
    /// requests never rebuild routes or copy schemas. Build once per server
    /// and clone the result.
    ///
    /// `max_response_bytes` caps the serialized size of each tool result; see
    /// [`McpConfig::max_response_bytes`](crate::config::McpConfig).
    pub(crate) fn new(database: Database, max_response_bytes: usize) -> Self {
        let mut router = Self::font_router();
        for route in router.map.values_mut() {
            close_tool_schemas(&mut route.attr);
        }
        let advertised = router.list_all();
        Self(Arc::new(Inner {
            database: Arc::new(database),
            router,
            advertised,
            max_response_bytes,
        }))
    }
}

impl AppTools {
    /// The shared tool router, schemas already closed.
    #[cfg(test)]
    pub(crate) fn router(&self) -> &ToolRouter<Self> {
        &self.0.router
    }
}

impl Deref for AppTools {
    type Target = Inner;

    fn deref(&self) -> &Inner {
        &self.0
    }
}
