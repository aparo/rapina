//! MCP (Model Context Protocol) support for Rapina.
//!
//! This module enables exposing your Rapina application as an MCP server so
//! that AI coding tools (Claude, Cursor, Copilot, etc.) can call your API
//! endpoints as structured tools.
//!
//! # Usage
//!
//! 1. Annotate functions with `#[mcp_tool]`:
//!
//! ```rust,ignore
//! use rapina::prelude::*;
//!
//! #[derive(Deserialize, JsonSchema)]
//! pub struct SearchParams {
//!     pub query: String,
//! }
//!
//! #[mcp_tool(description = "Search for users by name")]
//! async fn search_users(params: SearchParams, db: State<Db>) -> Vec<UserSummary> {
//!     // ...
//! }
//! ```
//!
//! 2. Enable the MCP HTTP endpoint in the app builder:
//!
//! ```rust,ignore
//! Rapina::new()
//!     .discover()
//!     .mcp()                 // serves at /__rapina/mcp
//!     // or .mcp_at("/mcp") // custom path
//!     .listen("127.0.0.1:3000")
//!     .await
//! ```
//!
//! 3. For stdio transport (`mcp-stdio` feature):
//!
//! ```rust,ignore
//! Rapina::new()
//!     .discover()
//!     .serve_mcp_stdio()
//!     .await
//! ```

pub mod protocol;
#[cfg(feature = "mcp-stdio")]
pub mod stdio;

use std::pin::Pin;
use std::sync::Arc;

use bytes::Bytes;
use http::{Request, Response, StatusCode, header::CONTENT_TYPE};
use http_body_util::BodyExt;
use hyper::body::Incoming;
use serde_json::Value;

use crate::extract::PathParams;
use crate::response::{APPLICATION_JSON, BoxBody, full};
use crate::state::AppState;

use protocol::{
    INTERNAL_ERROR, INVALID_PARAMS, METHOD_NOT_FOUND, PARSE_ERROR, PROTOCOL_VERSION,
    JsonRpcRequest, JsonRpcResponse,
};

/// Boxed future type for MCP tool handle functions.
pub type McpBoxFuture = Pin<Box<dyn std::future::Future<Output = Value> + Send + 'static>>;

/// Type alias for MCP tool handle function pointers.
///
/// Takes the deserialized MCP `arguments` JSON and the app state,
/// returns the tool result as a JSON value.
pub type McpHandleFn = fn(Value, Arc<AppState>) -> McpBoxFuture;

/// Type alias for MCP tool input schema function pointers.
pub type McpSchemaFn = fn() -> Value;

/// Risk level associated with invoking an MCP tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToolRisk {
    #[default]
    Read,
    Write,
    Destructive,
}

/// Whether the MCP client must ask for confirmation before invoking this tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToolConfirmation {
    #[default]
    Never,
    Required,
}

/// Static metadata about an MCP tool, collected at link time via `inventory`.
///
/// Emitted by `#[mcp_tool]` macros. When the MCP server starts, it collects
/// all descriptors from inventory and builds the tool registry.
pub struct McpToolDescriptor {
    /// The tool name used by MCP clients to call this tool.
    pub name: &'static str,
    /// Human-readable description shown to the AI model.
    pub description: &'static str,
    /// Risk level of this tool (used for MCP annotations).
    pub risk: ToolRisk,
    /// Whether the AI must confirm before calling this tool.
    pub confirmation: ToolConfirmation,
    /// Whether calling this tool multiple times with the same args is safe.
    pub idempotent: bool,
    /// Returns the JSON Schema for the tool's input arguments.
    pub input_schema: McpSchemaFn,
    /// Calls the tool with the given arguments and app state.
    pub handle: McpHandleFn,
}

inventory::collect!(McpToolDescriptor);

/// A resolved MCP tool entry with the input schema pre-computed.
pub struct McpToolEntry {
    pub name: &'static str,
    pub description: &'static str,
    pub risk: ToolRisk,
    pub confirmation: ToolConfirmation,
    pub idempotent: bool,
    /// Pre-computed JSON Schema (called once at startup).
    pub input_schema: Value,
    pub handle: McpHandleFn,
}

/// MCP tool registry, built from inventory at startup and stored in `AppState`.
pub struct McpRegistry {
    pub tools: Vec<McpToolEntry>,
}

impl McpRegistry {
    /// Build the registry by iterating all `McpToolDescriptor` instances from inventory.
    pub fn from_inventory() -> Self {
        let tools = inventory::iter::<McpToolDescriptor>
            .into_iter()
            .map(|d| McpToolEntry {
                name: d.name,
                description: d.description,
                risk: d.risk,
                confirmation: d.confirmation,
                idempotent: d.idempotent,
                input_schema: (d.input_schema)(),
                handle: d.handle,
            })
            .collect();
        Self { tools }
    }

    pub fn find_tool(&self, name: &str) -> Option<&McpToolEntry> {
        self.tools.iter().find(|t| t.name == name)
    }

    /// Build the `tools/list` result payload.
    pub fn tool_list_json(&self) -> Value {
        let tools: Vec<_> = self
            .tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "inputSchema": t.input_schema,
                    "annotations": {
                        "readOnlyHint": t.risk == ToolRisk::Read,
                        "destructiveHint": t.risk == ToolRisk::Destructive,
                        "idempotentHint": t.idempotent,
                        "openWorldHint": false,
                    }
                })
            })
            .collect();
        serde_json::json!({ "tools": tools })
    }
}

/// Path configuration for the MCP HTTP endpoint (stored in `AppState`).
#[derive(Clone)]
pub struct McpConfig {
    pub path: String,
}

impl McpConfig {
    pub fn new(path: impl Into<String>) -> Self {
        Self { path: path.into() }
    }
}

/// Dispatch a single JSON-RPC request against the MCP registry.
pub(crate) async fn dispatch(
    req: JsonRpcRequest,
    registry: &McpRegistry,
    state: &Arc<AppState>,
) -> Option<JsonRpcResponse> {
    let id = req.id.clone();

    // Notifications (no id) get no response.
    let is_notification = id.is_none();

    let response = match req.method.as_str() {
        "initialize" => {
            let result = serde_json::json!({
                "protocolVersion": PROTOCOL_VERSION,
                "serverInfo": {
                    "name": "rapina",
                    "version": env!("CARGO_PKG_VERSION"),
                },
                "capabilities": {
                    "tools": {}
                }
            });
            JsonRpcResponse::ok(id, result)
        }
        "initialized" => {
            // Notification — no response.
            return None;
        }
        "ping" => JsonRpcResponse::ok(id, serde_json::json!({})),
        "tools/list" => JsonRpcResponse::ok(id, registry.tool_list_json()),
        "tools/call" => {
            let params = req.params.unwrap_or(Value::Null);
            let name = match params.get("name").and_then(|v| v.as_str()) {
                Some(n) => n.to_owned(),
                None => {
                    return Some(JsonRpcResponse::err(
                        id,
                        INVALID_PARAMS,
                        "missing required field: name",
                    ));
                }
            };
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or(serde_json::json!({}));

            match registry.find_tool(&name) {
                None => JsonRpcResponse::err(
                    id,
                    INVALID_PARAMS,
                    format!("unknown tool: {name}"),
                ),
                Some(tool) => {
                    let result = (tool.handle)(arguments, state.clone()).await;
                    let content = serde_json::json!([{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&result)
                            .unwrap_or_else(|_| "null".to_string())
                    }]);
                    JsonRpcResponse::ok(
                        id,
                        serde_json::json!({
                            "content": content,
                            "isError": false
                        }),
                    )
                }
            }
        }
        _ => {
            if is_notification {
                return None;
            }
            JsonRpcResponse::err(id, METHOD_NOT_FOUND, format!("method not found: {}", req.method))
        }
    };

    Some(response)
}

/// HTTP transport handler for the MCP endpoint.
///
/// Registered at `POST /__rapina/mcp` (or a custom path) when the `mcp` feature
/// is enabled and `.mcp()` is called on the app builder.
pub async fn mcp_http_handler(
    req: Request<Incoming>,
    _params: PathParams,
    state: Arc<AppState>,
) -> Response<BoxBody> {
    let body_bytes = match req.into_body().collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(_) => {
            return json_response(
                StatusCode::BAD_REQUEST,
                &JsonRpcResponse::err(None, PARSE_ERROR, "failed to read request body"),
            );
        }
    };

    let rpc_req: JsonRpcRequest = match serde_json::from_slice(&body_bytes) {
        Ok(r) => r,
        Err(e) => {
            return json_response(
                StatusCode::BAD_REQUEST,
                &JsonRpcResponse::err(None, PARSE_ERROR, format!("invalid JSON: {e}")),
            );
        }
    };

    let registry = match state.get::<McpRegistry>() {
        Some(r) => r,
        None => {
            return json_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &JsonRpcResponse::err(None, INTERNAL_ERROR, "MCP registry not initialised"),
            );
        }
    };

    match dispatch(rpc_req, registry, &state).await {
        Some(response) => json_response(StatusCode::OK, &response),
        // Notification: respond with 202 Accepted, no body.
        None => Response::builder()
            .status(StatusCode::ACCEPTED)
            .body(crate::response::empty())
            .unwrap(),
    }
}

fn json_response(status: StatusCode, body: &impl serde::Serialize) -> Response<BoxBody> {
    let bytes = serde_json::to_vec(body).unwrap_or_default();
    Response::builder()
        .status(status)
        .header(CONTENT_TYPE, APPLICATION_JSON)
        .header("mcp-session-id", uuid::Uuid::new_v4().to_string())
        .body(full(Bytes::from(bytes)))
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_registry(tools: Vec<McpToolEntry>) -> McpRegistry {
        McpRegistry { tools }
    }

    fn make_state() -> Arc<AppState> {
        Arc::new(AppState::new())
    }

    fn parse_req(s: &str) -> JsonRpcRequest {
        serde_json::from_str(s).unwrap()
    }

    #[tokio::test]
    async fn test_initialize_returns_protocol_version() {
        let registry = make_registry(vec![]);
        let state = make_state();
        let req = parse_req(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{}}}"#,
        );
        let resp = dispatch(req, &registry, &state).await.unwrap();
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        assert_eq!(result["protocolVersion"], PROTOCOL_VERSION);
    }

    #[tokio::test]
    async fn test_tools_list_empty() {
        let registry = make_registry(vec![]);
        let state = make_state();
        let req = parse_req(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#);
        let resp = dispatch(req, &registry, &state).await.unwrap();
        assert!(resp.error.is_none());
        let tools = resp.result.unwrap()["tools"].as_array().unwrap().len();
        assert_eq!(tools, 0);
    }

    #[tokio::test]
    async fn test_ping_returns_empty_object() {
        let registry = make_registry(vec![]);
        let state = make_state();
        let req = parse_req(r#"{"jsonrpc":"2.0","id":3,"method":"ping"}"#);
        let resp = dispatch(req, &registry, &state).await.unwrap();
        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), serde_json::json!({}));
    }

    #[tokio::test]
    async fn test_unknown_method_returns_error() {
        let registry = make_registry(vec![]);
        let state = make_state();
        let req = parse_req(r#"{"jsonrpc":"2.0","id":4,"method":"foo/bar"}"#);
        let resp = dispatch(req, &registry, &state).await.unwrap();
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, METHOD_NOT_FOUND);
    }

    #[tokio::test]
    async fn test_initialized_notification_returns_none() {
        let registry = make_registry(vec![]);
        let state = make_state();
        let req = parse_req(r#"{"jsonrpc":"2.0","method":"initialized"}"#);
        let resp = dispatch(req, &registry, &state).await;
        assert!(resp.is_none());
    }

    #[tokio::test]
    async fn test_tools_call_unknown_tool() {
        let registry = make_registry(vec![]);
        let state = make_state();
        let req =
            parse_req(r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"no_such_tool","arguments":{}}}"#);
        let resp = dispatch(req, &registry, &state).await.unwrap();
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, INVALID_PARAMS);
    }

    #[tokio::test]
    async fn test_tools_call_missing_name() {
        let registry = make_registry(vec![]);
        let state = make_state();
        let req =
            parse_req(r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"arguments":{}}}"#);
        let resp = dispatch(req, &registry, &state).await.unwrap();
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, INVALID_PARAMS);
    }

    #[tokio::test]
    async fn test_tools_list_with_tool() {
        fn schema() -> Value {
            serde_json::json!({"type":"object","properties":{"q":{"type":"string"}}})
        }
        fn handle(
            _args: Value,
            _state: Arc<AppState>,
        ) -> Pin<Box<dyn std::future::Future<Output = Value> + Send + 'static>> {
            Box::pin(async { serde_json::json!({"results": []}) })
        }

        let entry = McpToolEntry {
            name: "search",
            description: "Search something",
            risk: ToolRisk::Read,
            confirmation: ToolConfirmation::Never,
            idempotent: true,
            input_schema: schema(),
            handle,
        };
        let registry = make_registry(vec![entry]);
        let state = make_state();
        let req = parse_req(r#"{"jsonrpc":"2.0","id":7,"method":"tools/list"}"#);
        let resp = dispatch(req, &registry, &state).await.unwrap();
        let result = resp.result.unwrap();
        let tools = result["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "search");
    }

    #[tokio::test]
    async fn test_tools_call_invokes_handle() {
        fn schema() -> Value {
            serde_json::json!({"type":"object"})
        }
        fn handle(
            _args: Value,
            _state: Arc<AppState>,
        ) -> Pin<Box<dyn std::future::Future<Output = Value> + Send + 'static>> {
            Box::pin(async { serde_json::json!(42) })
        }

        let entry = McpToolEntry {
            name: "the_answer",
            description: "Returns 42",
            risk: ToolRisk::Read,
            confirmation: ToolConfirmation::Never,
            idempotent: true,
            input_schema: schema(),
            handle,
        };
        let registry = make_registry(vec![entry]);
        let state = make_state();
        let req =
            parse_req(r#"{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"the_answer","arguments":{}}}"#);
        let resp = dispatch(req, &registry, &state).await.unwrap();
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        assert_eq!(result["isError"], false);
        // Content is a JSON-stringified version of 42
        let text = result["content"][0]["text"].as_str().unwrap();
        assert_eq!(text.trim(), "42");
    }
}
