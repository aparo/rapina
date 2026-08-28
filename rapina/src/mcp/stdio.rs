//! MCP stdio transport.
//!
//! Implements the newline-delimited JSON-RPC 2.0 transport for MCP over
//! standard input/output, suitable for running rapina as an MCP stdio server
//! (e.g. as a subprocess configured in an AI tool's MCP settings).
//!
//! # Usage
//!
//! Call [`serve`] instead of `Rapina::listen()` to run the application in
//! stdio mode. The function reads JSON-RPC requests line-by-line from stdin
//! and writes responses line-by-line to stdout.
//!
//! ```rust,ignore
//! use rapina::prelude::*;
//!
//! #[tokio::main]
//! async fn main() -> std::io::Result<()> {
//!     Rapina::new()
//!         .discover()
//!         .serve_mcp_stdio()
//!         .await
//! }
//! ```

use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::mcp::{McpRegistry, dispatch};
use crate::mcp::protocol::{PARSE_ERROR, JsonRpcRequest, JsonRpcResponse};
use crate::state::AppState;

/// Run an MCP stdio server loop.
///
/// Reads newline-delimited JSON-RPC 2.0 requests from stdin and writes
/// responses to stdout. Returns when stdin is closed (EOF).
pub async fn serve(registry: &McpRegistry, state: Arc<AppState>) -> std::io::Result<()> {
    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin);
    let mut stdout = tokio::io::stdout();
    let mut line = String::new();

    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            // EOF — client closed the connection.
            break;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let response: Option<JsonRpcResponse> = match serde_json::from_str::<JsonRpcRequest>(trimmed) {
            Ok(req) => dispatch(req, registry, &state).await,
            Err(e) => Some(JsonRpcResponse::err(
                None,
                PARSE_ERROR,
                format!("invalid JSON: {e}"),
            )),
        };

        if let Some(resp) = response {
            let mut out = serde_json::to_vec(&resp).unwrap_or_default();
            out.push(b'\n');
            stdout.write_all(&out).await?;
            stdout.flush().await?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::{McpToolEntry, ToolConfirmation, ToolRisk};
    use serde_json::Value;
    use std::pin::Pin;

    fn make_registry() -> McpRegistry {
        McpRegistry { tools: vec![] }
    }

    fn make_state() -> Arc<AppState> {
        Arc::new(AppState::new())
    }

    #[tokio::test]
    async fn test_stdio_initialize_roundtrip() {
        let registry = make_registry();
        let state = make_state();

        let input = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{}}}"#;
        let req: JsonRpcRequest = serde_json::from_str(input).unwrap();
        let resp = dispatch(req, &registry, &state).await.unwrap();

        let json = serde_json::to_string(&resp).unwrap();
        let parsed: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["jsonrpc"], "2.0");
        assert_eq!(parsed["id"], 1);
        assert!(parsed["result"]["protocolVersion"].is_string());
    }

    #[tokio::test]
    async fn test_stdio_tools_list() {
        fn schema() -> Value {
            serde_json::json!({"type":"object"})
        }
        fn handle(
            _args: Value,
            _state: Arc<AppState>,
        ) -> Pin<Box<dyn std::future::Future<Output = Value> + Send + 'static>> {
            Box::pin(async { serde_json::json!({"ok":true}) })
        }

        let registry = McpRegistry {
            tools: vec![McpToolEntry {
                name: "hello",
                description: "Say hello",
                risk: ToolRisk::Read,
                confirmation: ToolConfirmation::Never,
                idempotent: true,
                input_schema: schema(),
                handle,
            }],
        };
        let state = make_state();

        let req: JsonRpcRequest =
            serde_json::from_str(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#).unwrap();
        let resp = dispatch(req, &registry, &state).await.unwrap();
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: Value = serde_json::from_str(&json).unwrap();
        let tools = parsed["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "hello");
    }
}
