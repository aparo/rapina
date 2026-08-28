+++
title = "MCP Server"
description = "Expose your Rapina application as an MCP (Model Context Protocol) server for AI tools"
weight = 14
date = 2026-08-13
+++

Rapina can expose your application as an **MCP (Model Context Protocol) server**, letting AI coding tools like Claude Code, Cursor, and GitHub Copilot call your API endpoints directly as structured tools — without screen-scraping, curl commands, or manual schema discovery.

MCP defines a standard JSON-RPC 2.0 protocol for AI tools to discover and invoke backend capabilities. Once enabled, your rapina app is a first-class MCP server that any MCP-compatible client can connect to.

Two transports are supported:

- **HTTP** (`mcp` feature) — serves an MCP endpoint alongside your normal HTTP server, reachable over the network.
- **stdio** (`mcp-stdio` feature) — runs the process as a subprocess that communicates over standard input/output, the standard way to configure local MCP servers in AI tools.

## Enabling MCP

Add the feature to your `Cargo.toml`:

```toml
[dependencies]
# HTTP transport only:
rapina = { version = "0.14.0", features = ["mcp"] }

# Both HTTP and stdio:
rapina = { version = "0.14.0", features = ["mcp", "mcp-stdio"] }
```

## Defining MCP Tools

Annotate any async function with `#[mcp_tool]` to expose it as an MCP tool:

```rust
use rapina::prelude::*;

#[derive(Deserialize, JsonSchema)]
pub struct SearchParams {
    pub query: String,
    pub limit: Option<u32>,
}

#[mcp_tool(
    name = "search_users",
    description = "Search for users by name. Returns a list of matching user summaries.",
    risk = "read",
    idempotent = true,
)]
async fn search_users(params: SearchParams, db: State<Db>) -> Vec<UserSummary> {
    db.search_users(&params.query, params.limit.unwrap_or(10)).await
}
```

The macro handles everything:

- **Input schema** — derived automatically from `SearchParams` via `schemars::JsonSchema`, sent to the AI tool in `tools/list` so it knows what arguments to pass.
- **DI injection** — `State<Db>` (and any other `FromRequestParts` extractor) is resolved from `AppState` at call time.
- **Result serialization** — the return value is serialized to JSON and wrapped in the MCP `content` format.
- **Inventory registration** — `#[mcp_tool]` submits to `inventory` so the tool is discovered at startup alongside your routes.

### Function Signature Rules

```
async fn tool_name([input: InputType,] [di_arg: State<T>, ...]) -> impl Serialize
```

| Position | Type | Description |
|---|---|---|
| First arg (optional) | `InputType: Deserialize + JsonSchema` | The tool's input — deserialized from the MCP `arguments` JSON. Omit or use a `State<T>` first arg for tools with no input. |
| Remaining args | `State<T>`, `Db`, or any `FromRequestParts` | Dependency injection from `AppState`. |
| Return type | `impl Serialize` or `Result<impl Serialize, impl Display>` | Serialized to JSON for the MCP response. `Result` is unwrapped: `Ok` values are returned as-is, `Err` values become `{"error": "...message..."}`. |

**Tools with no input parameters** — if the first argument is a `State<T>` extractor (not a plain data type), there's no input schema and the tool accepts no arguments:

```rust
#[mcp_tool(description = "List all active users")]
async fn list_users(db: State<Db>) -> Vec<User> {
    db.list_active_users().await
}
```

**Tools with fallible results**:

```rust
#[mcp_tool(description = "Get user by ID")]
async fn get_user(params: GetUserParams, db: State<Db>) -> Result<User, Error> {
    db.find_user(params.id).await.ok_or(Error::not_found("user"))
}
```

### `#[mcp_tool]` Parameters

| Parameter | Type | Default | Description |
|---|---|---|---|
| `name` | `"string"` | function name | Stable tool name sent to the AI client. Use a consistent, descriptive name — AI tools cache tool lists. |
| `description` | `"string"` | `""` | Human-readable description shown to the AI model when it decides which tool to call. Be specific about what the tool does and what it returns. |
| `risk` | `"read"` \| `"write"` \| `"destructive"` | `"read"` | Indicates the side-effect level. Clients may use this to filter or warn before calling. |
| `confirmation` | `"never"` \| `"required"` | `"never"` | When `"required"`, MCP clients that support it will ask the user to confirm before calling the tool. |
| `idempotent` | `true` \| `false` | `false` | Marks the tool safe to call multiple times with the same arguments without different effects. |

## HTTP Transport

Enable the MCP endpoint on the app builder:

```rust
use rapina::prelude::*;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    Rapina::new()
        .discover()   // required: discovers #[mcp_tool] functions
        .mcp()        // serves at /__rapina/mcp
        .listen("127.0.0.1:3000")
        .await
}
```

Use `.mcp_at(path)` for a custom path:

```rust
Rapina::new()
    .discover()
    .mcp_at("/api/mcp")
    .listen("127.0.0.1:3000")
    .await
```

The endpoint handles `POST` requests carrying a JSON-RPC 2.0 payload. It implements the [MCP Streamable HTTP transport](https://modelcontextprotocol.io/specification/2024-11-05/basic/transports#streamable-http) (protocol version `2024-11-05`).

### Supported Methods

| Method | Description |
|---|---|
| `initialize` | Handshake — returns server name, version, and capabilities. |
| `initialized` | Notification from the client that initialization is complete. No response. |
| `ping` | Liveness check — returns an empty object. |
| `tools/list` | Returns the list of all registered MCP tools with their input schemas and annotations. |
| `tools/call` | Calls a tool by name with the given arguments. |

### Authentication

The MCP endpoint follows rapina's normal authentication rules. If JWT auth is enabled, the endpoint requires a valid `Authorization: Bearer <token>` header by default. Mark it public to allow unauthenticated access:

```rust
Rapina::new()
    .with_auth(auth_config)
    .public_route("POST", "/__rapina/mcp")
    .discover()
    .mcp()
    .listen("127.0.0.1:3000")
    .await
```

## Stdio Transport

The stdio transport runs rapina as a subprocess that communicates over stdin/stdout. This is how most desktop AI tools (Claude Code, Cursor, etc.) connect to local MCP servers — they launch the binary and talk to it directly without a network connection.

Call `.serve_mcp_stdio().await` instead of `.listen()`:

```rust
use rapina::prelude::*;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    Rapina::new()
        .discover()
        .serve_mcp_stdio()
        .await
}
```

`serve_mcp_stdio()` reads newline-delimited JSON-RPC 2.0 requests from stdin and writes responses to stdout. It returns when stdin is closed (the AI tool terminated the subprocess).

### Configuring Claude Code

Add the server to `~/.claude/mcp_servers.json` (or your project's `.claude/mcp_servers.json`):

```json
{
  "my-api": {
    "command": "cargo",
    "args": ["run", "--bin", "my-api", "--features", "mcp-stdio"],
    "cwd": "/path/to/my-project"
  }
}
```

Or if you have a compiled binary:

```json
{
  "my-api": {
    "command": "/path/to/my-api-binary"
  }
}
```

The `#[mcp_tool]` functions appear as tools in the Claude Code interface immediately after saving the config.

## Full Example

A complete application exposing user search and creation as MCP tools:

```rust
use rapina::prelude::*;
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

#[derive(Deserialize, JsonSchema)]
pub struct SearchParams {
    pub query: String,
    pub limit: Option<u32>,
}

#[derive(Serialize)]
pub struct UserSummary {
    pub id: u64,
    pub name: String,
    pub email: String,
}

#[mcp_tool(
    name = "search_users",
    description = "Search for users by name or email. Returns matching users with their IDs.",
    risk = "read",
    idempotent = true,
)]
async fn search_users(params: SearchParams, db: State<Db>) -> Result<Vec<UserSummary>, Error> {
    Ok(db.search_users(&params.query, params.limit.unwrap_or(10)).await?)
}

#[derive(Deserialize, JsonSchema)]
pub struct CreateUserParams {
    pub name: String,
    pub email: String,
}

#[mcp_tool(
    name = "create_user",
    description = "Create a new user account. Returns the created user with their assigned ID.",
    risk = "write",
    confirmation = "required",
)]
async fn create_user(params: CreateUserParams, db: State<Db>) -> Result<UserSummary, Error> {
    Ok(db.create_user(&params.name, &params.email).await?)
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // HTTP mode: run alongside the normal API server
    Rapina::new()
        .with_database(DatabaseConfig::from_env()?).await?
        .discover()
        .mcp()         // /__rapina/mcp
        .listen("127.0.0.1:3000")
        .await
}
```

For stdio mode, swap the last block:

```rust
#[tokio::main]
async fn main() -> std::io::Result<()> {
    Rapina::new()
        .with_database(DatabaseConfig::from_env()?).await?
        .discover()
        .serve_mcp_stdio()
        .await
}
```

## How It Works

When `.discover()` is called, rapina collects all `McpToolDescriptor` instances submitted to `inventory` by `#[mcp_tool]` macros. Each descriptor contains the tool name, description, a function pointer for the input schema, and a function pointer for the call handler.

At startup (when `.mcp()` is configured), the descriptors are resolved into an `McpRegistry` stored in `AppState`. The HTTP handler reads the registry from state, dispatches the JSON-RPC request, and calls the matching tool's handler with the provided arguments.

The tool handler uses the same DI mechanism as background jobs: synthetic `Request` parts are created, and `FromRequestParts` extractors are called against `AppState` to resolve dependencies like `State<Db>`.

## Comparison with llms.txt and OpenAPI

| | llms.txt | OpenAPI | MCP |
|---|---|---|---|
| **Audience** | AI tools reading docs | Developers, API clients | AI tools calling tools |
| **Interaction** | Read-only discovery | Client-initiated HTTP | AI-initiated function calls |
| **Schema** | Markdown narrative | JSON Schema | JSON Schema (per tool) |
| **Auth** | Public endpoint | Standard HTTP auth | Standard HTTP auth (or subprocess) |
| **Best for** | Route documentation | API contracts | AI agent integration |

Use all three together for the best experience: OpenAPI and llms.txt help the AI understand your API structure, while MCP lets it take action.
