use proc_macro::TokenStream;
use quote::quote;
use syn::spanned::Spanned;
use syn::ItemFn;

mod config;
mod job;
mod relay;
mod route;
mod schema;

use route::route_macro;

/// Registers a GET route handler.
///
/// # Syntax
///
/// ```ignore
/// #[get("/users")]
/// async fn list_users() -> Json<Vec<User>> { /* ... */ }
///
/// // Single path parameter:
/// #[get("/users/:id")]
/// async fn get_user(id: Path<u64>) -> Json<User> { /* ... */ }
///
/// // Multiple path parameters — tuple, positional (left to right in pattern):
/// #[get("/orgs/:org_id/teams/:team_id")]
/// async fn get_team(Path((org_id, team_id)): Path<(u64, u64)>) -> Json<Team> { /* ... */ }
///
/// // With a group prefix (registers at /api/users):
/// #[get("/users", group = "/api")]
/// async fn list_users() -> Json<Vec<User>> { /* ... */ }
///
/// // With extended OpenAPI metadata:
/// #[get(
///     "/users",
///     id = "users.list",
///     summary = "List all users",
///     tags = ["users"],
///     deprecated = false,
/// )]
/// async fn list_users() -> Json<Vec<User>> { /* ... */ }
/// ```
///
/// # OpenAPI parameters
///
/// | Parameter | Type | Description |
/// |---|---|---|
/// | `group` | `&str` | Path prefix joined at compile time |
/// | `description` | `&str` | Long description (also falls back to rustdoc `///`) |
/// | `id` | `&str` | Stable `operationId` override (default: handler name) |
/// | `summary` | `&str` | Short one-line summary (default: humanized handler name) |
/// | `tags` | `[&str, ...]` | Tags for grouping operations in Swagger UI |
/// | `deprecated` | `bool` | Mark the operation deprecated (`deprecated = true`) |
///
/// The `group` parameter joins the prefix with the path at compile time,
/// so the handler is registered at the full path during auto-discovery.
#[proc_macro_attribute]
pub fn get(attr: TokenStream, item: TokenStream) -> TokenStream {
    route_macro("GET", attr, item)
}

/// Registers a POST route handler.
///
/// Supports the same parameters as [`get`]: `group`, `description`, `id`,
/// `summary`, `tags`, `deprecated`.
///
/// # Example
///
/// ```ignore
/// #[post(
///     "/users",
///     id = "users.create",
///     summary = "Create a user",
///     tags = ["users"],
/// )]
/// async fn create_user(body: Json<CreateUserRequest>) -> Json<User> { /* ... */ }
/// ```
#[proc_macro_attribute]
pub fn post(attr: TokenStream, item: TokenStream) -> TokenStream {
    route_macro("POST", attr, item)
}

/// Registers a PUT route handler.
///
/// Supports the same parameters as [`get`]: `group`, `description`, `id`,
/// `summary`, `tags`, `deprecated`.
#[proc_macro_attribute]
pub fn put(attr: TokenStream, item: TokenStream) -> TokenStream {
    route_macro("PUT", attr, item)
}

/// Registers a PATCH route handler.
///
/// Supports the same parameters as [`get`]: `group`, `description`, `id`,
/// `summary`, `tags`, `deprecated`.
///
/// # Example
///
/// ```ignore
/// #[patch("/users/:id")]
/// async fn update_user(Path(id): Path<u64>) -> Json<User> { /* ... */ }
/// ```
#[proc_macro_attribute]
pub fn patch(attr: TokenStream, item: TokenStream) -> TokenStream {
    route_macro("PATCH", attr, item)
}

/// Registers a DELETE route handler.
///
/// Supports the same parameters as [`get`]: `group`, `description`, `id`,
/// `summary`, `tags`, `deprecated`.
#[proc_macro_attribute]
pub fn delete(attr: TokenStream, item: TokenStream) -> TokenStream {
    route_macro("DELETE", attr, item)
}

/// Marks a route as public (no authentication required).
///
/// When authentication is enabled via `Rapina::with_auth()`, all routes
/// require a valid JWT token by default. Use `#[public]` to allow
/// unauthenticated access to specific routes.
///
/// # Example
///
/// ```ignore
/// use rapina::prelude::*;
///
/// #[public]
/// #[get("/health")]
/// async fn health() -> &'static str {
///     "ok"
/// }
///
/// #[public]
/// #[post("/login")]
/// async fn login(body: Json<LoginRequest>) -> Result<Json<TokenResponse>> {
///     // ... authenticate and return token
/// }
/// ```
///
/// Note: Routes starting with `/__rapina` are automatically public.
#[proc_macro_attribute]
pub fn public(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let func: ItemFn = syn::parse(item.clone()).expect("#[public] must be applied to a function");
    let func_name_str = func.sig.ident.to_string();
    let item2: proc_macro2::TokenStream = item.into();
    quote! {
        #item2
        rapina::inventory::submit! {
            rapina::discovery::PublicMarker {
                handler_name: #func_name_str,
            }
        }
    }
    .into()
}



/// Registers a channel handler for the relay system.
///
/// Channel handlers receive [`RelayEvent`](rapina::relay::RelayEvent) events
/// when clients subscribe, send messages, or disconnect from matching topics.
///
/// The pattern supports exact matches and prefix matches (trailing `*`):
///
/// - `"chat:lobby"` — matches only the exact topic `"chat:lobby"`
/// - `"room:*"` — matches any topic starting with `"room:"`
///
/// The first parameter must be `RelayEvent`. Remaining parameters are
/// extracted via `FromRequestParts` with synthetic request parts (same
/// extractors as HTTP handlers, minus body extractors).
///
/// # Example
///
/// ```ignore
/// use rapina::prelude::*;
/// use rapina::relay::{Relay, RelayEvent};
///
/// #[relay("room:*")]
/// async fn room(event: RelayEvent, relay: Relay) -> Result<()> {
///     match &event {
///         RelayEvent::Join { topic, conn_id } => {
///             relay.track(topic, *conn_id, serde_json::json!({}));
///         }
///         RelayEvent::Message { topic, event: ev, payload, .. } => {
///             relay.push(topic, ev, payload).await?;
///         }
///         RelayEvent::Leave { topic, conn_id } => {
///             relay.untrack(topic, *conn_id);
///         }
///     }
///     Ok(())
/// }
/// ```
#[proc_macro_attribute]
pub fn relay(attr: TokenStream, item: TokenStream) -> TokenStream {
    relay::relay_macro_impl(attr.into(), item.into()).into()
}

/// Marks a static Prometheus collector for auto-discovery.
///
/// Annotate a module-level `static` holding a collector and `.discover()`
/// registers it with the `/metrics` endpoint, so you don't have to thread it
/// through `add_metric()`. Requires the `metrics` feature plus both
/// `.enable_metrics()` and `.discover()` on the app builder.
///
/// The collector type must be `Clone` (all built-in prometheus types are;
/// clones share the same underlying values). Wrap the collector in
/// `std::sync::LazyLock` or `once_cell::sync::Lazy`; no built-in prometheus
/// type can be constructed in a const context, so a bare static won't
/// compile. `OnceLock`-style cells are not supported, and the static must
/// live at module scope, not inside a function body.
///
/// This is the only Rapina attribute applied to a `static` rather than a
/// function.
///
/// # Example
///
/// ```ignore
/// use std::sync::LazyLock;
/// use rapina::metric;
/// use rapina::prometheus::IntCounter;
///
/// #[metric]
/// static ORDERS_TOTAL: LazyLock<IntCounter> = LazyLock::new(|| {
///     IntCounter::new("orders_total", "Total orders placed").unwrap()
/// });
/// ```
#[proc_macro_attribute]
pub fn metric(attr: TokenStream, item: TokenStream) -> TokenStream {
    metric_macro_impl(attr.into(), item.into()).into()
}

fn metric_macro_impl(
    attr: proc_macro2::TokenStream,
    item: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    if !attr.is_empty() {
        return syn::Error::new_spanned(attr, "#[metric] does not take arguments")
            .to_compile_error();
    }
    let item = match syn::parse2::<syn::ItemStatic>(item) {
        Ok(item) => item,
        Err(err) => {
            return syn::Error::new(
                err.span(),
                "#[metric] can only be applied to a `static` item",
            )
            .to_compile_error();
        }
    };
    if let syn::StaticMutability::Mut(m) = &item.mutability {
        return syn::Error::new_spanned(m, "#[metric] cannot be applied to a `static mut`")
            .to_compile_error();
    }

    let ident = &item.ident;
    let collector_fn = quote::format_ident!("__rapina_metric_{}", ident);

    quote! {
        #item

        #[doc(hidden)]
        #[allow(non_snake_case)]
        fn #collector_fn() -> Box<dyn rapina::prometheus::core::Collector> {
            Box::new(#ident.clone())
        }

        rapina::inventory::submit! {
            rapina::discovery::MetricDescriptor {
                collector: #collector_fn,
            }
        }
    }
}

/// Defines a background job handler.
///
/// Annotate an `async fn` to register it as a background job. The first
/// argument is always the payload type (must implement `Serialize +
/// DeserializeOwned`). Remaining arguments are dependency-injected from
/// `AppState` — `State<T>` and `Db` are the supported extractors.
///
/// Optionally configure the queue and retry limit:
///
/// ```text
/// #[job(queue = "emails", max_retries = 5)]
/// ```
///
/// Defaults: `queue = "default"`, `max_retries = 3`.
///
/// # What the macro generates
///
/// Given:
///
/// ```rust,ignore
/// #[job(queue = "emails")]
/// async fn send_welcome_email(
///     payload: WelcomeEmailPayload,
///     mailer: State<Mailer>,
/// ) -> JobResult { ... }
/// ```
///
/// The macro generates a helper function with the same name and visibility:
///
/// ```rust,ignore
/// fn send_welcome_email(payload: WelcomeEmailPayload) -> JobRequest {
///     JobRequest { job_type: "send_welcome_email", queue: "emails", ... }
/// }
/// ```
///
/// The `Jobs` extractor and `enqueue()` API for dispatching jobs from handlers
/// are planned for a follow-up release.
///
/// The handler is also registered via `inventory` for runtime dispatch —
/// no manual registration needed.
///
/// # Feature requirement
///
/// Requires the `database` feature. The generated types (`JobRequest`,
/// `JobDescriptor`) live in `rapina::jobs`, which is gated behind that feature.
///
/// # DI limitations
///
/// Only `State<T>` and `Db` work in job handlers. Request-bound extractors
/// (`Context`, `Headers`, `Path`, `CurrentUser`) will fail at runtime.
#[proc_macro_attribute]
pub fn job(attr: TokenStream, item: TokenStream) -> TokenStream {
    job::job_macro_impl(attr.into(), item.into()).into()
}

/// Derive macro for type-safe configuration
///
/// Generates a `from_env()` method that loads configuration from environment variables.
#[proc_macro_derive(Config, attributes(env, default))]
pub fn derive_config(input: TokenStream) -> TokenStream {
    config::derive_config_impl(input.into()).into()
}

/// Define database entities with Prisma-like syntax.
///
/// This macro generates SeaORM entity definitions from a declarative syntax
/// where types indicate relationships. Each entity automatically gets `id`,
/// `created_at`, and `updated_at` fields.
///
/// # Syntax
///
/// ```ignore
/// rapina::schema! {
///     User {
///         email: String,
///         name: String,
///         posts: Vec<Post>,        // has_many relationship
///     }
///
///     Post {
///         title: String,
///         content: Text,           // TEXT column type
///         author: User,            // belongs_to -> generates author_id
///         comments: Vec<Comment>,
///     }
///
///     Comment {
///         content: Text,
///         post: Post,
///         author: Option<User>,    // optional belongs_to
///     }
/// }
/// ```
///
/// # Generated Code
///
/// For each entity, the macro generates a SeaORM module with:
/// - `Model` struct with auto `id`, `created_at`, `updated_at`
/// - `Relation` enum with proper SeaORM attributes
/// - `Related<T>` trait implementations
/// - `ActiveModelBehavior` implementation
///
/// # Supported Types
///
/// | Schema Type | Rust Type | Notes |
/// |-------------|-----------|-------|
/// | `String` | `String` | Default varchar |
/// | `Text` | `String` | TEXT column |
/// | `i32` | `i32` | |
/// | `i64` | `i64` | |
/// | `f32` | `f32` | |
/// | `f64` | `f64` | |
/// | `bool` | `bool` | |
/// | `Uuid` | `Uuid` | |
/// | `DateTime` | `DateTimeUtc` | |
/// | `Date` | `Date` | |
/// | `Decimal` | `Decimal` | |
/// | `Json` | `Json` | |
/// | `Option<T>` | `Option<T>` | Nullable |
/// | `Vec<Entity>` | - | has_many relationship |
/// | `Entity` | - | belongs_to (generates FK) |
#[proc_macro]
pub fn schema(input: TokenStream) -> TokenStream {
    schema::schema_impl(input.into()).into()
}

#[cfg(test)]
mod tests {
    use super::metric_macro_impl;
    use crate::job::job_macro_impl;
    use crate::relay::relay_macro_impl;
    use quote::quote;

    #[test]
    fn test_metric_macro_generates_collector_fn_and_inventory() {
        let input = quote! {
            static ORDERS_TOTAL: LazyLock<IntCounter> = LazyLock::new(|| {
                IntCounter::new("orders_total", "Total orders placed").unwrap()
            });
        };

        let output = metric_macro_impl(quote!(), input);
        let output_str = output.to_string();

        assert!(output_str.contains("static ORDERS_TOTAL"));
        assert!(output_str.contains("__rapina_metric_ORDERS_TOTAL"));
        assert!(output_str.contains("inventory :: submit !"));
        assert!(output_str.contains("MetricDescriptor"));
    }

    #[test]
    fn test_metric_macro_rejects_args() {
        let input = quote! {
            static ORDERS_TOTAL: LazyLock<IntCounter> = LazyLock::new(make_counter);
        };

        let output_str = metric_macro_impl(quote!(name = "orders"), input).to_string();

        assert!(output_str.contains("compile_error !"));
        assert!(output_str.contains("does not take arguments"));
    }

    #[test]
    fn test_metric_macro_rejects_fn() {
        let input = quote! {
            fn not_a_static() {}
        };

        let output_str = metric_macro_impl(quote!(), input).to_string();

        assert!(output_str.contains("compile_error !"));
        assert!(output_str.contains("can only be applied to a `static` item"));
    }

    #[test]
    fn test_metric_macro_rejects_static_mut() {
        let input = quote! {
            static mut ORDERS_TOTAL: IntCounter = make_counter();
        };

        let output_str = metric_macro_impl(quote!(), input).to_string();

        assert!(output_str.contains("compile_error !"));
        assert!(output_str.contains("cannot be applied to a `static mut`"));
    }

    #[test]
    fn test_relay_macro_extracts_additional_params() {
        let attr = quote!("room:*");
        let input = quote! {
            async fn room(
                event: rapina::relay::RelayEvent,
                relay: rapina::relay::Relay,
                log: rapina::extract::State<TestLog>,
            ) -> Result<(), rapina::error::Error> {
                Ok(())
            }
        };

        let output = relay_macro_impl(attr, input);
        let output_str = output.to_string();

        // Both extractors should use FromRequestParts
        assert!(output_str.contains("let relay ="));
        assert!(output_str.contains("let log ="));
        assert!(output_str.contains("FromRequestParts"));
    }

    // -- #[job] retry policy attributes --

    fn minimal_job_fn() -> proc_macro2::TokenStream {
        quote! {
            async fn my_job(payload: String) {}
        }
    }

    #[test]
    fn job_macro_defaults_retry_policy_and_delay() {
        let output = job_macro_impl(quote! {}, minimal_job_fn()).to_string();
        assert!(
            output.contains("retry_policy : \"exponential\""),
            "default retry_policy should be exponential"
        );
        assert!(
            output.contains("retry_delay_secs : 1f64"),
            "default retry_delay_secs should be 1.0"
        );
    }

    #[test]
    fn job_macro_fixed_retry_policy() {
        let output =
            job_macro_impl(quote! { retry_policy = "fixed" }, minimal_job_fn()).to_string();
        assert!(output.contains("retry_policy : \"fixed\""));
    }

    #[test]
    fn job_macro_none_retry_policy() {
        let output = job_macro_impl(quote! { retry_policy = "none" }, minimal_job_fn()).to_string();
        assert!(output.contains("retry_policy : \"none\""));
    }

    #[test]
    fn job_macro_retry_delay_float_literal() {
        let output =
            job_macro_impl(quote! { retry_delay_secs = 30.0 }, minimal_job_fn()).to_string();
        assert!(output.contains("retry_delay_secs : 30f64"));
    }

    #[test]
    fn job_macro_retry_delay_integer_literal() {
        let output = job_macro_impl(quote! { retry_delay_secs = 30 }, minimal_job_fn()).to_string();
        assert!(output.contains("retry_delay_secs : 30f64"));
    }

    #[test]
    fn job_macro_invalid_retry_policy_is_compile_error() {
        let output =
            job_macro_impl(quote! { retry_policy = "random" }, minimal_job_fn()).to_string();
        assert!(output.contains("compile_error"));
        assert!(
            output.contains("exponential") || output.contains("fixed") || output.contains("none")
        );
    }

    #[test]
    fn job_macro_unknown_attr_error_mentions_retry_attrs() {
        let output = job_macro_impl(quote! { retries = 3 }, minimal_job_fn()).to_string();
        assert!(output.contains("compile_error"));
        assert!(output.contains("retry_policy"));
        assert!(output.contains("retry_delay_secs"));
    }

    #[test]
    fn job_macro_all_retry_attrs_combined() {
        let output = job_macro_impl(
            quote! { retry_policy = "fixed", retry_delay_secs = 15, max_retries = 5 },
            minimal_job_fn(),
        )
        .to_string();
        assert!(output.contains("retry_policy : \"fixed\""));
        assert!(output.contains("retry_delay_secs : 15f64"));
    }

}

// ─── #[mcp_tool] ─────────────────────────────────────────────────────────────

/// Expose an async function as an MCP (Model Context Protocol) tool.
///
/// Annotated functions are collected via `inventory` and served by the MCP
/// HTTP endpoint (`/__rapina/mcp`) or via stdio when the respective feature is
/// enabled and configured on the app builder.
///
/// # Parameters
///
/// | Parameter | Type | Default | Description |
/// |---|---|---|---|
/// | `name` | `"string"` | function name | Stable tool name used by MCP clients |
/// | `description` | `"string"` | `""` | Human-readable description shown to the AI |
/// | `risk` | `"read"` \| `"write"` \| `"destructive"` | `"read"` | Risk level (MCP annotation) |
/// | `confirmation` | `"never"` \| `"required"` | `"never"` | Whether the AI must confirm before calling |
/// | `idempotent` | `true` \| `false` | `false` | Whether repeated calls with same args are safe |
///
/// # Function signature
///
/// The function must be `async`. If it accepts arguments, the **first**
/// argument is the input struct (must implement `serde::Deserialize` and
/// `schemars::JsonSchema`). All remaining arguments are DI extractors
/// resolved from `AppState` (e.g. `State<MyService>`).
///
/// The return type must implement `serde::Serialize`. If the return type is
/// named `Result`, both the success value (must be `Serialize`) and the
/// error (must be `Display`) are handled: errors are returned as a JSON
/// object `{"error": "...message..."}`.
///
/// # Example
///
/// ```rust,ignore
/// use rapina::prelude::*;
/// use rapina::mcp_tool;
///
/// #[derive(serde::Deserialize, schemars::JsonSchema)]
/// pub struct SearchParams {
///     pub query: String,
///     pub limit: Option<u32>,
/// }
///
/// #[mcp_tool(
///     name = "search_users",
///     description = "Search for users by name",
///     risk = "read",
///     idempotent = true,
/// )]
/// async fn search_users(params: SearchParams, db: State<Db>) -> Vec<UserSummary> {
///     db.search_users(&params.query, params.limit.unwrap_or(10)).await
/// }
/// ```
#[proc_macro_attribute]
pub fn mcp_tool(attr: TokenStream, item: TokenStream) -> TokenStream {
    mcp_tool_impl(attr.into(), item.into()).into()
}

struct McpToolAttr {
    name: Option<String>,
    description: String,
    risk: String,
    confirmation: String,
    idempotent: bool,
}

impl Default for McpToolAttr {
    fn default() -> Self {
        Self {
            name: None,
            description: String::new(),
            risk: "read".to_string(),
            confirmation: "never".to_string(),
            idempotent: false,
        }
    }
}

impl syn::parse::Parse for McpToolAttr {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut attr = McpToolAttr::default();

        while !input.is_empty() {
            let ident: syn::Ident = input.parse()?;
            input.parse::<syn::Token![=]>()?;

            if ident == "name" {
                let lit: syn::LitStr = input.parse()?;
                attr.name = Some(lit.value());
            } else if ident == "description" {
                let lit: syn::LitStr = input.parse()?;
                attr.description = lit.value();
            } else if ident == "risk" {
                let lit: syn::LitStr = input.parse()?;
                let val = lit.value();
                if !matches!(val.as_str(), "read" | "write" | "destructive") {
                    return Err(syn::Error::new(
                        lit.span(),
                        "risk must be \"read\", \"write\", or \"destructive\"",
                    ));
                }
                attr.risk = val;
            } else if ident == "confirmation" {
                let lit: syn::LitStr = input.parse()?;
                let val = lit.value();
                if !matches!(val.as_str(), "never" | "required") {
                    return Err(syn::Error::new(
                        lit.span(),
                        "confirmation must be \"never\" or \"required\"",
                    ));
                }
                attr.confirmation = val;
            } else if ident == "idempotent" {
                let lit: syn::LitBool = input.parse()?;
                attr.idempotent = lit.value();
            } else {
                return Err(syn::Error::new(
                    ident.span(),
                    format!(
                        "unknown #[mcp_tool] attribute `{ident}` — supported: `name`, `description`, `risk`, `confirmation`, `idempotent`"
                    ),
                ));
            }

            if input.peek(syn::Token![,]) {
                input.parse::<syn::Token![,]>()?;
            }
        }

        Ok(attr)
    }
}

/// Returns `true` when the last path segment of a return type is named "Result".
fn return_type_is_result(ret: &syn::ReturnType) -> bool {
    if let syn::ReturnType::Type(_, ty) = ret {
        if let syn::Type::Path(tp) = ty.as_ref() {
            if let Some(seg) = tp.path.segments.last() {
                return seg.ident == "Result";
            }
        }
    }
    false
}

/// Returns `true` when the first FnArg looks like a DI extractor (State<T>, Db, etc.)
/// rather than an input payload type.
fn arg_is_di_extractor(arg: &syn::FnArg) -> bool {
    if let syn::FnArg::Typed(pt) = arg {
        if let syn::Type::Path(tp) = pt.ty.as_ref() {
            if let Some(seg) = tp.path.segments.first() {
                let name = seg.ident.to_string();
                // Known DI extractor types: any type whose name starts with "State"
                // or is a well-known shorthand like "Db".
                return name == "State" || name == "Db";
            }
        }
    }
    false
}

fn mcp_tool_impl(
    attr: proc_macro2::TokenStream,
    item: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let mcp_attr: McpToolAttr = match syn::parse2(attr) {
        Ok(a) => a,
        Err(e) => return e.to_compile_error(),
    };

    let func: syn::ItemFn = match syn::parse2(item) {
        Ok(f) => f,
        Err(e) => return e.to_compile_error(),
    };

    if func.sig.asyncness.is_none() {
        return syn::Error::new(
            func.sig.fn_token.span,
            "#[mcp_tool] must be applied to an async function",
        )
        .to_compile_error();
    }

    if !func.sig.generics.params.is_empty() {
        return syn::Error::new(
            func.sig.generics.params.first().unwrap().span(),
            "#[mcp_tool] does not support generic type parameters",
        )
        .to_compile_error();
    }

    let func_name = &func.sig.ident;
    let func_name_str = func_name.to_string();
    let func_vis = &func.vis;
    let func_attrs = &func.attrs;
    let func_block = &func.block;
    let impl_inputs = &func.sig.inputs;
    let impl_output = &func.sig.output;

    let tool_name_str = mcp_attr.name.unwrap_or_else(|| func_name_str.clone());
    let description_str = &mcp_attr.description;
    let idempotent = mcp_attr.idempotent;

    let risk_tokens = match mcp_attr.risk.as_str() {
        "write" => quote! { rapina::mcp::ToolRisk::Write },
        "destructive" => quote! { rapina::mcp::ToolRisk::Destructive },
        _ => quote! { rapina::mcp::ToolRisk::Read },
    };

    let confirmation_tokens = match mcp_attr.confirmation.as_str() {
        "required" => quote! { rapina::mcp::ToolConfirmation::Required },
        _ => quote! { rapina::mcp::ToolConfirmation::Never },
    };

    let impl_fn_name = syn::Ident::new(
        &format!("__rapina_mcp_impl_{}", func_name_str),
        proc_macro2::Span::call_site(),
    );
    let handle_fn_name = syn::Ident::new(
        &format!("__rapina_mcp_handle_{}", func_name_str),
        proc_macro2::Span::call_site(),
    );
    let schema_fn_name = syn::Ident::new(
        &format!("__rapina_mcp_schema_{}", func_name_str),
        proc_macro2::Span::call_site(),
    );

    let args: Vec<_> = func.sig.inputs.iter().collect();

    // Determine which args are input vs DI:
    // - If 0 args: no input, no DI.
    // - If first arg looks like a DI extractor: all args are DI, no input type.
    // - Otherwise: first arg is input type, remaining are DI.
    let (has_input, input_type, di_args) = if args.is_empty() {
        (false, None, vec![])
    } else if arg_is_di_extractor(args[0]) {
        (false, None, args.clone())
    } else {
        let input_ty = match &args[0] {
            syn::FnArg::Typed(pt) => Some(&pt.ty),
            syn::FnArg::Receiver(r) => {
                return syn::Error::new(
                    r.self_token.span,
                    "#[mcp_tool] cannot be applied to a method — use a free function",
                )
                .to_compile_error();
            }
        };
        (true, input_ty, args[1..].to_vec())
    };

    // Build DI extraction code for remaining args.
    let mut extractor_extractions = Vec::new();
    let mut di_call_args = Vec::new();

    for (i, arg) in di_args.iter().enumerate() {
        if let syn::FnArg::Typed(pat_type) = arg {
            let arg_type = &pat_type.ty;
            let tmp = syn::Ident::new(
                &format!("__rapina_mcp_di_{}", i),
                proc_macro2::Span::call_site(),
            );
            extractor_extractions.push(quote! {
                let #tmp = match <#arg_type as rapina::extract::FromRequestParts>::from_request_parts(
                    &__rapina_parts, &__rapina_params, &__rapina_state
                ).await {
                    Ok(v) => v,
                    Err(e) => return rapina::serde_json::json!({
                        "error": format!("dependency injection failed: {}", e)
                    }),
                };
            });
            di_call_args.push(quote! { #tmp });
        }
    }

    // Build the deserialization code for the input arg (if present).
    let input_deser = if has_input {
        let ty = input_type.unwrap();
        quote! {
            let __rapina_mcp_input: #ty = match rapina::serde_json::from_value(__rapina_mcp_args) {
                Ok(v) => v,
                Err(e) => return rapina::serde_json::json!({
                    "error": format!("invalid arguments: {}", e)
                }),
            };
        }
    } else {
        quote! {}
    };

    // Build the call args list (input arg first if present, then DI).
    let call_input = if has_input {
        quote! { __rapina_mcp_input, }
    } else {
        quote! {}
    };

    // Build the result serialization code.
    // If return type is Result, unwrap Ok/Err separately for cleaner output.
    let result_to_json = if return_type_is_result(impl_output) {
        quote! {
            match __rapina_mcp_result {
                Ok(v) => rapina::serde_json::to_value(v)
                    .unwrap_or(rapina::serde_json::Value::Null),
                Err(e) => rapina::serde_json::json!({ "error": e.to_string() }),
            }
        }
    } else {
        quote! {
            rapina::serde_json::to_value(__rapina_mcp_result)
                .unwrap_or(rapina::serde_json::Value::Null)
        }
    };

    // Build input schema function.
    let schema_impl = if has_input {
        let ty = input_type.unwrap();
        quote! {
            fn #schema_fn_name() -> rapina::serde_json::Value {
                rapina::openapi_schema_for::<#ty>()
                    .and_then(|s| rapina::serde_json::to_value(s).ok())
                    .unwrap_or_else(|| rapina::serde_json::json!({"type": "object"}))
            }
        }
    } else {
        quote! {
            fn #schema_fn_name() -> rapina::serde_json::Value {
                rapina::serde_json::json!({"type": "object", "properties": {}})
            }
        }
    };

    quote! {
        // Original function body, renamed internally. Never called directly by users.
        #(#func_attrs)*
        #[doc(hidden)]
        async fn #impl_fn_name(#impl_inputs) #impl_output
        #func_block

        // DI wrapper registered in inventory.
        #[doc(hidden)]
        fn #handle_fn_name(
            __rapina_mcp_args: rapina::serde_json::Value,
            __rapina_state: std::sync::Arc<rapina::state::AppState>,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = rapina::serde_json::Value> + Send + 'static>>
        {
            Box::pin(async move {
                #input_deser
                let (__rapina_parts, _) = rapina::http::Request::new(()).into_parts();
                let __rapina_params = rapina::extract::PathParams::new();
                #(#extractor_extractions)*
                let __rapina_mcp_result = #impl_fn_name(#call_input #(#di_call_args),*).await;
                #result_to_json
            })
        }

        // Input schema function.
        #[doc(hidden)]
        #schema_impl

        // Public re-export with the original name (as a no-op unit struct for discovery).
        // This keeps the user-visible name in scope for IDE completion.
        #func_vis use #impl_fn_name as #func_name;

        rapina::inventory::submit! {
            rapina::mcp::McpToolDescriptor {
                name: #tool_name_str,
                description: #description_str,
                risk: #risk_tokens,
                confirmation: #confirmation_tokens,
                idempotent: #idempotent,
                input_schema: #schema_fn_name,
                handle: #handle_fn_name,
            }
        }
    }
}

#[cfg(test)]
mod mcp_tool_tests {
    use super::*;

    #[test]
    fn test_mcp_tool_basic_generates_descriptor() {
        let attr = quote::quote! { description = "A test tool" };
        let item = quote::quote! {
            async fn my_tool() -> &'static str { "hello" }
        };
        let output = mcp_tool_impl(attr, item).to_string();
        assert!(output.contains("McpToolDescriptor"));
        assert!(output.contains("\"my_tool\""));
        assert!(output.contains("A test tool"));
    }

    #[test]
    fn test_mcp_tool_custom_name() {
        let attr = quote::quote! { name = "custom_name", description = "desc" };
        let item = quote::quote! {
            async fn my_tool() -> &'static str { "hello" }
        };
        let output = mcp_tool_impl(attr, item).to_string();
        assert!(output.contains("\"custom_name\""));
    }

    #[test]
    fn test_mcp_tool_risk_write() {
        let attr = quote::quote! { description = "d", risk = "write" };
        let item = quote::quote! {
            async fn my_tool() -> &'static str { "hello" }
        };
        let output = mcp_tool_impl(attr, item).to_string();
        assert!(output.contains("ToolRisk :: Write"));
    }

    #[test]
    fn test_mcp_tool_confirmation_required() {
        let attr = quote::quote! { description = "d", confirmation = "required" };
        let item = quote::quote! {
            async fn my_tool() -> &'static str { "hello" }
        };
        let output = mcp_tool_impl(attr, item).to_string();
        assert!(output.contains("ToolConfirmation :: Required"));
    }

    #[test]
    fn test_mcp_tool_idempotent_true() {
        let attr = quote::quote! { description = "d", idempotent = true };
        let item = quote::quote! {
            async fn my_tool() -> &'static str { "hello" }
        };
        let output = mcp_tool_impl(attr, item).to_string();
        assert!(output.contains("idempotent : true"));
    }

    #[test]
    fn test_mcp_tool_with_input_type() {
        let attr = quote::quote! { description = "search" };
        let item = quote::quote! {
            async fn search_tool(params: SearchParams) -> Vec<String> { vec![] }
        };
        let output = mcp_tool_impl(attr, item).to_string();
        assert!(output.contains("SearchParams"));
        assert!(output.contains("from_value"));
        assert!(output.contains("openapi_schema_for"));
    }

    #[test]
    fn test_mcp_tool_di_only_no_input_deser() {
        let attr = quote::quote! { description = "list" };
        let item = quote::quote! {
            async fn list_tool(db: State<Db>) -> Vec<String> { vec![] }
        };
        let output = mcp_tool_impl(attr, item).to_string();
        // No from_value call since there's no input type
        assert!(!output.contains("from_value"));
        // But State<Db> should be extracted via FromRequestParts
        assert!(output.contains("from_request_parts"));
    }

    #[test]
    fn test_mcp_tool_result_return_uses_ok_err_match() {
        let attr = quote::quote! { description = "fallible" };
        let item = quote::quote! {
            async fn fallible_tool() -> Result<String, MyError> { Ok("ok".into()) }
        };
        let output = mcp_tool_impl(attr, item).to_string();
        // Result-returning functions should use match Ok/Err pattern
        assert!(output.contains("to_string"));
    }

    #[test]
    fn test_mcp_tool_invalid_risk_is_compile_error() {
        let attr = quote::quote! { risk = "unknown" };
        let item = quote::quote! {
            async fn my_tool() -> &'static str { "hello" }
        };
        let output = mcp_tool_impl(attr, item).to_string();
        assert!(output.contains("compile_error"));
    }

    #[test]
    fn test_mcp_tool_unknown_attr_is_compile_error() {
        let attr = quote::quote! { foo = "bar" };
        let item = quote::quote! {
            async fn my_tool() -> &'static str { "hello" }
        };
        let output = mcp_tool_impl(attr, item).to_string();
        assert!(output.contains("compile_error"));
    }

    #[test]
    fn test_mcp_tool_non_async_is_compile_error() {
        let attr = quote::quote! { description = "sync" };
        let item = quote::quote! {
            fn sync_tool() -> &'static str { "hello" }
        };
        let output = mcp_tool_impl(attr, item).to_string();
        assert!(output.contains("compile_error"));
    }
}
