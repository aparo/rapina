use proc_macro::TokenStream;
use quote::quote;
use syn::spanned::Spanned;
use syn::{FnArg, ItemFn, LitStr, Pat};

/// Parsed route macro attribute: `"/path"`, `"/path", group = "/prefix"`,
/// `"/path", description = "..."`, `"/path", id = "op.id"`,
/// `"/path", summary = "Short description"`, `"/path", tags = ["tag1", "tag2"]`,
/// `"/path", deprecated = true`, or any combination thereof.
struct RouteAttr {
    path: LitStr,
    group: Option<LitStr>,
    description: Option<LitStr>,
    /// Stable OpenAPI operationId override (e.g. "users.create").
    id: Option<LitStr>,
    /// Short one-line summary for OpenAPI (overrides the auto-generated humanized name).
    summary: Option<LitStr>,
    /// OpenAPI tags for grouping operations (e.g. `["users", "admin"]`).
    tags: Vec<LitStr>,
    /// Mark this operation as deprecated in the OpenAPI spec.
    deprecated: bool,
}

impl syn::parse::Parse for RouteAttr {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let path: LitStr = input.parse()?;
        let mut group: Option<LitStr> = None;
        let mut description: Option<LitStr> = None;
        let mut id: Option<LitStr> = None;
        let mut summary: Option<LitStr> = None;
        let mut tags: Vec<LitStr> = Vec::new();
        let mut deprecated = false;

        while input.peek(syn::Token![,]) {
            input.parse::<syn::Token![,]>()?;
            if input.is_empty() {
                break;
            }
            let ident: syn::Ident = input.parse()?;
            input.parse::<syn::Token![=]>()?;
            if ident == "group" {
                let value: LitStr = input.parse()?;
                group = Some(value);
            } else if ident == "description" {
                let value: LitStr = input.parse()?;
                description = Some(value);
            } else if ident == "id" {
                let value: LitStr = input.parse()?;
                id = Some(value);
            } else if ident == "summary" {
                let value: LitStr = input.parse()?;
                summary = Some(value);
            } else if ident == "tags" {
                let content;
                syn::bracketed!(content in input);
                while !content.is_empty() {
                    let s: LitStr = content.parse()?;
                    tags.push(s);
                    if content.peek(syn::Token![,]) {
                        content.parse::<syn::Token![,]>()?;
                    }
                }
            } else if ident == "deprecated" {
                let value: syn::LitBool = input.parse()?;
                deprecated = value.value();
            } else {
                return Err(syn::Error::new(
                    ident.span(),
                    "expected `group`, `description`, `id`, `summary`, `tags`, or `deprecated`",
                ));
            }
        }

        if !input.is_empty() {
            return Err(input.error("unexpected tokens after route attribute"));
        }
        Ok(RouteAttr {
            path,
            group,
            description,
            id,
            summary,
            tags,
            deprecated,
        })
    }
}

/// Join a group prefix with a route path at compile time.
fn join_paths(prefix: &str, path: &str) -> String {
    let prefix = prefix.trim_end_matches('/');
    if path.is_empty() || path == "/" {
        if prefix.is_empty() {
            return "/".to_string();
        }
        return prefix.to_string();
    }
    let path = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    format!("{prefix}{path}")
}

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

fn route_macro_core(
    method: &str,
    attr: proc_macro2::TokenStream,
    item: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let route_attr: RouteAttr = syn::parse2(attr).expect("expected path as string literal");
    let path_str = if let Some(ref group) = route_attr.group {
        let g = group.value();
        assert!(
            g.starts_with('/'),
            "group prefix must start with `/`, got: {g:?}"
        );
        join_paths(&g, &route_attr.path.value())
    } else {
        route_attr.path.value()
    };
    let mut func: ItemFn = syn::parse2(item).expect("expected function");

    let func_name = &func.sig.ident;
    let func_name_str = func_name.to_string();
    let func_vis = &func.vis;

    // Extract #[public] attribute if present (when #[public] is below the route macro)
    let is_public = extract_public_attr(&mut func.attrs);

    // Resolve description: explicit attr wins, then first rustdoc line, then None
    let description_value: Option<String> = route_attr
        .description
        .as_ref()
        .map(|l| l.value())
        .or_else(|| extract_doc_description(&func.attrs));

    let operation_id_value: Option<String> = route_attr.id.as_ref().map(|l| l.value());
    let summary_value: Option<String> = route_attr.summary.as_ref().map(|l| l.value());
    let tags_values: Vec<String> = route_attr.tags.iter().map(|l| l.value()).collect();
    let deprecated_value = route_attr.deprecated;

    // Extract #[errors(ErrorType)] attribute if present
    let error_type = extract_errors_attr(&mut func.attrs);

    // Extract #[cache(ttl = N)] attribute if present
    let cache_ttl = extract_cache_attr(&mut func.attrs);

    let error_responses_impl = if let Some(err_type) = &error_type {
        quote! {
            fn error_responses() -> Vec<rapina::error::ErrorVariant> {
                <#err_type as rapina::error::DocumentedError>::error_variants()
            }
        }
    } else {
        quote! {}
    };

    // Extract return type for schema generation
    let response_schema_impl = if let syn::ReturnType::Type(_, return_type) = &func.sig.output {
        if let Some(inner_type) = extract_json_inner_type(return_type) {
            quote! {
                fn response_schema() -> Option<serde_json::Value> {
                    Some(rapina::openapi_schema_for::<#inner_type>())
                }
            }
        } else {
            quote! {}
        }
    } else {
        quote! {}
    };

    // Extract request body type and content type for schema generation.
    // Only generate requestBody for POST, PUT, and PATCH methods per OpenAPI spec.
    let (request_schema_impl, request_content_type_impl, request_body_required_impl) =
        if matches!(method, "POST" | "PUT" | "PATCH") {
            if let Some(meta) = extract_request_body_meta(&func.sig.inputs) {
                let inner_type = meta.inner_type;
                let content_type = meta.content_type;
                let required = meta.required;
                (
                    quote! {
                        fn request_schema() -> Option<serde_json::Value> {
                            Some(rapina::openapi_schema_for::<#inner_type>())
                        }
                    },
                    quote! {
                        fn request_content_type() -> Option<&'static str> {
                            Some(#content_type)
                        }
                    },
                    quote! {
                        fn request_body_required() -> Option<bool> {
                            Some(#required)
                        }
                    },
                )
            } else {
                (quote! {}, quote! {}, quote! {})
            }
        } else {
            (quote! {}, quote! {}, quote! {})
        };

    // Collect header params (also strips #[header("name")] attrs from inputs)
    let header_params = match collect_header_params(&mut func.sig.inputs) {
        Ok(p) => p,
        Err(e) => return e.to_compile_error(),
    };

    // Build an index: arg_idx → &HeaderParamMeta for O(1) lookup during codegen
    let header_by_arg: std::collections::HashMap<usize, &HeaderParamMeta> =
        header_params.iter().map(|p| (p.arg_idx, p)).collect();

    // Build header_parameters() impl for the Handler trait
    let header_parameters_impl = if header_params.is_empty() {
        quote! {}
    } else {
        let entries = header_params.iter().map(|p| {
            let name = &p.name;
            let required = p.required;
            quote! {
                rapina::discovery::HeaderParamInfo {
                    name: #name.to_string(),
                    required: #required,
                }
            }
        });
        quote! {
            fn header_parameters() -> Vec<rapina::discovery::HeaderParamInfo> {
                vec![#(#entries),*]
            }
        }
    };

    // Build description() impl for the Handler trait
    let description_impl = if let Some(ref desc) = description_value {
        quote! {
            fn description() -> Option<&'static str> {
                Some(#desc)
            }
        }
    } else {
        quote! {}
    };

    // Build operation_id() impl — only when an explicit `id` was given
    let operation_id_impl = if let Some(ref op_id) = operation_id_value {
        quote! {
            fn operation_id() -> Option<&'static str> {
                Some(#op_id)
            }
        }
    } else {
        quote! {}
    };

    // Build summary() impl — only when an explicit `summary` was given
    let summary_impl = if let Some(ref s) = summary_value {
        quote! {
            fn summary() -> Option<&'static str> {
                Some(#s)
            }
        }
    } else {
        quote! {}
    };

    // Build tags() impl — only when tags were specified
    let tags_impl = if !tags_values.is_empty() {
        quote! {
            fn tags() -> &'static [&'static str] {
                &[#(#tags_values),*]
            }
        }
    } else {
        quote! {}
    };

    // Build deprecated() impl — only when deprecated = true
    let deprecated_impl = if deprecated_value {
        quote! {
            fn deprecated() -> bool {
                true
            }
        }
    } else {
        quote! {}
    };

    let args: Vec<_> = func.sig.inputs.iter().collect();

    // Extract return type for type annotation (helps with type inference in async blocks)
    let return_type_annotation = match &func.sig.output {
        syn::ReturnType::Type(_, ty) => quote! { : #ty },
        syn::ReturnType::Default => quote! {},
    };

    // Optional cache TTL header injection
    let cache_header_injection = if let Some(ttl) = cache_ttl {
        let ttl_str = ttl.to_string();
        quote! {
            let mut __rapina_response = __rapina_response;
            __rapina_response.headers_mut().insert(
                "x-rapina-cache-ttl",
                rapina::http::HeaderValue::from_static(#ttl_str),
            );
        }
    } else {
        quote! {}
    };

    // Build the handler body
    // Use __rapina_ prefix for internal variables to avoid shadowing user's variables
    let handler_body = if args.is_empty() {
        let inner_block = &func.block;
        quote! {
            let __rapina_result #return_type_annotation = (async #inner_block).await;
            let __rapina_response = rapina::response::IntoResponse::into_response(__rapina_result);
            #cache_header_injection
            __rapina_response
        }
    } else {
        let inner_block = &func.block;

        // Check if all args are header extractors (so we never need to split req into parts)
        let all_headers = args.iter().all(|arg| {
            if let FnArg::Typed(pt) = arg {
                detect_header_type(&pt.ty).is_some()
            } else {
                false
            }
        });

        // Check if the single arg is a header type
        let single_is_header = args.len() == 1
            && args.first().is_some_and(|arg| {
                if let FnArg::Typed(pt) = arg {
                    detect_header_type(&pt.ty).is_some()
                } else {
                    false
                }
            });

        if args.len() == 1 && !single_is_header {
            // Single non-header arg: pass request directly to FromRequest
            let arg = &args[0];
            if let FnArg::Typed(pat_type) = arg {
                let pat = &pat_type.pat;
                let arg_type = &pat_type.ty;
                let tmp = syn::Ident::new("__rapina_arg_0", proc_macro2::Span::call_site());
                quote! {
                    let #tmp = match <#arg_type as rapina::extract::FromRequest>::from_request(__rapina_req, &__rapina_params, &__rapina_state).await {
                        Ok(v) => v,
                        Err(e) => return rapina::response::IntoResponse::into_response(e),
                    };
                    let #pat = #tmp;
                    let __rapina_result #return_type_annotation = (async #inner_block).await;
                    let __rapina_response = rapina::response::IntoResponse::into_response(__rapina_result);
                    #cache_header_injection
                    __rapina_response
                }
            } else {
                unreachable!("handler argument must be a typed pattern")
            }
        } else if all_headers {
            // All args are header extractors — extract from parts, no body split needed
            let mut header_extractions = Vec::new();
            for (i, arg) in args.iter().enumerate() {
                if let FnArg::Typed(pat_type) = arg {
                    let pat = &pat_type.pat;
                    let tmp = syn::Ident::new(
                        &format!("__rapina_arg_{}", i),
                        proc_macro2::Span::call_site(),
                    );
                    let meta = header_by_arg.get(&i).expect("all_headers: missing meta");
                    header_extractions.push(gen_header_extraction(
                        &meta.inner_type,
                        meta.required,
                        &meta.name,
                        &tmp,
                    ));
                    header_extractions.push(quote! { let #pat = #tmp; });
                }
            }
            quote! {
                let (__rapina_parts, _) = __rapina_req.into_parts();
                #(#header_extractions)*
                let __rapina_result #return_type_annotation = (async #inner_block).await;
                let __rapina_response = rapina::response::IntoResponse::into_response(__rapina_result);
                #cache_header_injection
                __rapina_response
            }
        } else {
            // Multiple args: all but last use FromRequestParts (or header extraction), last uses FromRequest
            let mut parts_extractions = Vec::new();

            for (i, arg) in args[..args.len() - 1].iter().enumerate() {
                if let FnArg::Typed(pat_type) = arg {
                    let pat = &pat_type.pat;
                    let arg_type = &pat_type.ty;
                    let tmp = syn::Ident::new(
                        &format!("__rapina_arg_{}", i),
                        proc_macro2::Span::call_site(),
                    );
                    if detect_header_type(arg_type).is_some() {
                        let meta = header_by_arg.get(&i).expect("mixed: missing meta");
                        parts_extractions.push(gen_header_extraction(
                            &meta.inner_type,
                            meta.required,
                            &meta.name,
                            &tmp,
                        ));
                        parts_extractions.push(quote! { let #pat = #tmp; });
                    } else {
                        parts_extractions.push(quote! {
                            let #tmp = match <#arg_type as rapina::extract::FromRequestParts>::from_request_parts(&__rapina_parts, &__rapina_params, &__rapina_state).await {
                                Ok(v) => v,
                                Err(e) => return rapina::response::IntoResponse::into_response(e),
                            };
                            let #pat = #tmp;
                        });
                    }
                }
            }

            let last_arg = args.last().unwrap();
            let last_extraction = if let FnArg::Typed(pat_type) = last_arg {
                let pat = &pat_type.pat;
                let arg_type = &pat_type.ty;
                let last_idx = args.len() - 1;
                let tmp = syn::Ident::new(
                    &format!("__rapina_arg_{}", last_idx),
                    proc_macro2::Span::call_site(),
                );
                if detect_header_type(arg_type).is_some() {
                    let meta = header_by_arg
                        .get(&last_idx)
                        .expect("last arg: missing meta");
                    let header_extr =
                        gen_header_extraction(&meta.inner_type, meta.required, &meta.name, &tmp);
                    quote! {
                        #header_extr
                        let #pat = #tmp;
                        // Reconstruct the request (body not consumed for header-only last arg)
                        let _ = __rapina_body;
                    }
                } else {
                    quote! {
                        let __rapina_req = rapina::http::Request::from_parts(__rapina_parts, __rapina_body);
                        let #tmp = match <#arg_type as rapina::extract::FromRequest>::from_request(__rapina_req, &__rapina_params, &__rapina_state).await {
                            Ok(v) => v,
                            Err(e) => return rapina::response::IntoResponse::into_response(e),
                        };
                        let #pat = #tmp;
                    }
                }
            } else {
                unreachable!("handler argument must be a typed pattern")
            };

            quote! {
                let (__rapina_parts, __rapina_body) = __rapina_req.into_parts();
                #(#parts_extractions)*
                #last_extraction
                let __rapina_result #return_type_annotation = (async #inner_block).await;
                let __rapina_response = rapina::response::IntoResponse::into_response(__rapina_result);
                #cache_header_injection
                __rapina_response
            }
        }
    };

    // Build the router method call for the register function
    let router_method = syn::Ident::new(&method.to_lowercase(), proc_macro2::Span::call_site());
    let register_fn_name = syn::Ident::new(
        &format!("__rapina_register_{}", func_name_str),
        proc_macro2::Span::call_site(),
    );

    // Generate the struct, Handler impl, and inventory submission
    quote! {
        #[derive(Clone, Copy)]
        #[allow(non_camel_case_types)]
        #func_vis struct #func_name;

        impl rapina::handler::Handler for #func_name {
            const NAME: &'static str = #func_name_str;

            #response_schema_impl
            #request_schema_impl
            #request_content_type_impl
            #request_body_required_impl
            #error_responses_impl
            #header_parameters_impl
            #description_impl
            #operation_id_impl
            #summary_impl
            #tags_impl
            #deprecated_impl

            fn call(
                &self,
                __rapina_req: rapina::hyper::Request<rapina::hyper::body::Incoming>,
                __rapina_params: rapina::extract::PathParams,
                __rapina_state: std::sync::Arc<rapina::state::AppState>,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = rapina::hyper::Response<rapina::response::BoxBody>> + Send>> {
                Box::pin(async move {
                    #handler_body
                })
            }
        }

        #[doc(hidden)]
        fn #register_fn_name(__rapina_router: rapina::router::Router) -> rapina::router::Router {
            __rapina_router.#router_method(#path_str, #func_name)
        }

        rapina::inventory::submit! {
            rapina::discovery::RouteDescriptor {
                method: #method,
                path: #path_str,
                handler_name: #func_name_str,
                is_public: #is_public,
                response_schema: <#func_name as rapina::handler::Handler>::response_schema,
                request_schema: <#func_name as rapina::handler::Handler>::request_schema,
                request_content_type: <#func_name as rapina::handler::Handler>::request_content_type,
                request_body_required: <#func_name as rapina::handler::Handler>::request_body_required,
                error_responses: <#func_name as rapina::handler::Handler>::error_responses,
                header_parameters: <#func_name as rapina::handler::Handler>::header_parameters,
                description: <#func_name as rapina::handler::Handler>::description,
                register: #register_fn_name,
            }
        }
    }
}

/// Extracts the inner type from Json<T> wrapper for schema generation
fn extract_json_inner_type(return_type: &syn::Type) -> Option<proc_macro2::TokenStream> {
    if let syn::Type::Path(type_path) = return_type
        && let Some(last_segment) = type_path.path.segments.last()
    {
        // Direct Json<T>
        if last_segment.ident == "Json"
            && let syn::PathArguments::AngleBracketed(args) = &last_segment.arguments
            && let Some(syn::GenericArgument::Type(inner_type)) = args.args.first()
        {
            return Some(quote!(#inner_type));
        }

        // Result<Json<T>> or Result<Json<T>, E>
        if last_segment.ident == "Result"
            && let syn::PathArguments::AngleBracketed(args) = &last_segment.arguments
            && let Some(syn::GenericArgument::Type(ok_type)) = args.args.first()
        {
            return extract_json_inner_type(ok_type);
        }
    }
    None
}

/// Extracts the request body metadata from handler function arguments.
/// Supports Json<T>, Form<T>, Validated<Json<T>>, and Validated<Form<T>>.
fn extract_request_body_meta(
    inputs: &syn::punctuated::Punctuated<syn::FnArg, syn::Token![,]>,
) -> Option<RequestBodyMeta> {
    for arg in inputs.iter() {
        if let syn::FnArg::Typed(pat_type) = arg {
            if let Some(meta) = extract_body_inner_type(&pat_type.ty) {
                return Some(meta);
            }
        }
    }
    None
}

/// Information about a request body extractor.
struct RequestBodyMeta {
    inner_type: proc_macro2::TokenStream,
    content_type: &'static str,
    required: bool,
}

/// Extracts the inner type and content type from Json<T>, Form<T>, Validated<Json<T>>/Validated<Form<T>>,
/// or Option<Json<T>>/Option<Form<T>>.
fn extract_body_inner_type(ty: &syn::Type) -> Option<RequestBodyMeta> {
    if let syn::Type::Path(type_path) = ty
        && let Some(last_segment) = type_path.path.segments.last()
    {
        // Direct Json<T>
        if last_segment.ident == "Json"
            && let syn::PathArguments::AngleBracketed(args) = &last_segment.arguments
            && let Some(syn::GenericArgument::Type(inner_type)) = args.args.first()
        {
            return Some(RequestBodyMeta {
                inner_type: quote!(#inner_type),
                content_type: "application/json",
                required: true,
            });
        }
        // Direct Form<T>
        if last_segment.ident == "Form"
            && let syn::PathArguments::AngleBracketed(args) = &last_segment.arguments
            && let Some(syn::GenericArgument::Type(inner_type)) = args.args.first()
        {
            return Some(RequestBodyMeta {
                inner_type: quote!(#inner_type),
                content_type: "application/x-www-form-urlencoded",
                required: true,
            });
        }
        // Validated<Json<T>> or Validated<Form<T>>
        if last_segment.ident == "Validated"
            && let syn::PathArguments::AngleBracketed(args) = &last_segment.arguments
            && let Some(syn::GenericArgument::Type(inner_extractor)) = args.args.first()
        {
            return extract_body_inner_type(inner_extractor);
        }
        // Option<Json<T>> or Option<Form<T>> - optional request body
        if last_segment.ident == "Option"
            && let syn::PathArguments::AngleBracketed(args) = &last_segment.arguments
            && let Some(syn::GenericArgument::Type(inner_extractor)) = args.args.first()
        {
            if let Some(mut meta) = extract_body_inner_type(inner_extractor) {
                meta.required = false;
                return Some(meta);
            }
        }
    }
    None
}

/// Extract the first non-empty line from `///` doc comments on a function.
fn extract_doc_description(attrs: &[syn::Attribute]) -> Option<String> {
    for attr in attrs {
        if !attr.path().is_ident("doc") {
            continue;
        }
        if let syn::Meta::NameValue(nv) = &attr.meta {
            if let syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(s),
                ..
            }) = &nv.value
            {
                let line = s.value();
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
    }
    None
}

/// Extract #[errors(ErrorType)] attribute from function attributes, removing it if found.
fn extract_errors_attr(attrs: &mut Vec<syn::Attribute>) -> Option<syn::Type> {
    let idx = attrs
        .iter()
        .position(|attr| attr.path().is_ident("errors"))?;
    let attr = attrs.remove(idx);
    let err_type: syn::Type = attr.parse_args().expect("expected #[errors(ErrorType)]");
    Some(err_type)
}

/// Extract #[cache(ttl = N)] attribute from function attributes, removing it if found.
fn extract_cache_attr(attrs: &mut Vec<syn::Attribute>) -> Option<u64> {
    let idx = attrs
        .iter()
        .position(|attr| attr.path().is_ident("cache"))?;
    let attr = attrs.remove(idx);

    let mut ttl: Option<u64> = None;
    attr.parse_nested_meta(|meta| {
        if meta.path.is_ident("ttl") {
            let value = meta.value()?;
            let lit: syn::LitInt = value.parse()?;
            ttl = Some(lit.base10_parse()?);
            Ok(())
        } else {
            Err(meta.error("expected `ttl`"))
        }
    })
    .expect("expected #[cache(ttl = N)]");

    ttl
}

/// Extract #[public] attribute from function attributes, removing it if found.
fn extract_public_attr(attrs: &mut Vec<syn::Attribute>) -> bool {
    if let Some(idx) = attrs.iter().position(|attr| attr.path().is_ident("public")) {
        attrs.remove(idx);
        true
    } else {
        false
    }
}

/// Generate the extraction code for a `Header<T>` or `Option<Header<T>>` parameter.
///
/// `header_name` is the resolved HTTP header name (kebab-case, possibly from
/// an explicit `#[header("name")]` attribute).
fn gen_header_extraction(
    inner_type: &syn::Type,
    required: bool,
    header_name: &str,
    tmp: &syn::Ident,
) -> proc_macro2::TokenStream {
    if required {
        quote! {
            let #tmp = match rapina::extract::extract_header::<#inner_type>(&__rapina_parts, #header_name) {
                Ok(v) => rapina::extract::Header::new(#header_name, v),
                Err(e) => return rapina::response::IntoResponse::into_response(e),
            };
        }
    } else {
        quote! {
            let #tmp = match rapina::extract::extract_optional_header::<#inner_type>(&__rapina_parts, #header_name) {
                Ok(Some(v)) => Some(rapina::extract::Header::new(#header_name, v)),
                Ok(None) => None,
                Err(e) => return rapina::response::IntoResponse::into_response(e),
            };
        }
    }
}

/// Metadata about a single `Header<T>` or `Option<Header<T>>` parameter.
struct HeaderParamMeta {
    /// Zero-based index of this param in the handler's argument list.
    arg_idx: usize,
    /// The HTTP header name (e.g. "x-request-id").
    name: String,
    /// Whether the parameter is required (`Header<T>`) or optional (`Option<Header<T>>`).
    required: bool,
    /// The inner `T` type (for generating the extraction call).
    inner_type: syn::Type,
}

/// Extract `#[header("name")]` attribute from a parameter's attribute list.
///
/// Returns the explicit header name if present, removing the attribute.
fn extract_header_attr(attrs: &mut Vec<syn::Attribute>) -> Option<String> {
    let idx = attrs
        .iter()
        .position(|attr| attr.path().is_ident("header"))?;
    let attr = attrs.remove(idx);
    let lit: LitStr = attr.parse_args().expect("expected #[header(\"name\")]");
    Some(lit.value())
}

/// Detect if `ty` is `Header<T>` (required) or `Option<Header<T>>` (optional).
///
/// Returns `Some((inner_type, required))` on match, `None` otherwise.
///
/// Matches `Header<T>` (bare or path-qualified as `extract::Header<T>` /
/// `rapina::extract::Header<T>`).  Any other qualifying path (e.g.
/// `my_crate::Header<T>`) returns `None`, so user-defined types named `Header`
/// fall through to normal handling instead of producing a confusing compile
/// error from macro-generated code.
fn detect_header_type(ty: &syn::Type) -> Option<(syn::Type, bool)> {
    let syn::Type::Path(type_path) = ty else {
        return None;
    };
    let last = type_path.path.segments.last()?;

    // Direct Header<T>
    if last.ident == "Header" {
        // When the type is qualified (e.g. `foo::Header`), only treat it as
        // rapina's Header if the leading path is a known rapina prefix.
        // Bare `Header` (imported via prelude) has no leading segments and
        // is always accepted.
        let segments: Vec<_> = type_path.path.segments.iter().collect();
        let is_rapina_header = match segments.len() {
            1 => true,                                                            // bare `Header`
            2 => segments[0].ident == "extract", // `extract::Header`
            3 => segments[0].ident == "rapina" && segments[1].ident == "extract", // `rapina::extract::Header`
            _ => false,
        };
        if is_rapina_header {
            if let syn::PathArguments::AngleBracketed(args) = &last.arguments {
                if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                    return Some((inner.clone(), true));
                }
            }
        }
    }

    // Option<Header<T>>
    if last.ident == "Option" {
        if let syn::PathArguments::AngleBracketed(args) = &last.arguments {
            if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                if let Some((inner_t, _)) = detect_header_type(inner) {
                    return Some((inner_t, false));
                }
            }
        }
    }

    None
}

/// Collect all `Header<T>` / `Option<Header<T>>` parameters from handler inputs.
///
/// Also strips any `#[header("name")]` attributes from the parameters
/// (they are not valid Rust attributes and must be removed before codegen).
fn collect_header_params(
    inputs: &mut syn::punctuated::Punctuated<syn::FnArg, syn::Token![,]>,
) -> syn::Result<Vec<HeaderParamMeta>> {
    let mut params = Vec::new();

    for (arg_idx, arg) in inputs.iter_mut().enumerate() {
        let syn::FnArg::Typed(pat_type) = arg else {
            continue;
        };

        let Some((inner_type, required)) = detect_header_type(&pat_type.ty) else {
            continue;
        };

        // Check for explicit #[header("name")] override on the parameter
        let explicit_name = extract_header_attr(&mut pat_type.attrs);

        // Derive header name from snake_case param name, or use explicit override.
        let name = if let Some(n) = explicit_name {
            n
        } else if let Pat::Ident(pat_ident) = &*pat_type.pat {
            pat_ident.ident.to_string().to_kebab_case()
        } else {
            // Destructure pattern — can't infer name, user must use #[header("name")]
            return Err(syn::Error::new_spanned(
                &*pat_type.pat,
                "Header<T> parameter with a destructure pattern must have a #[header(\"name\")] attribute",
            ));
        };

        params.push(HeaderParamMeta {
            arg_idx,
            name,
            required,
            inner_type,
        });
    }

    Ok(params)
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

    #[test]
    fn test_no_cache_attr_no_ttl_header() {
        let path = quote!("/products");
        let input = quote! {
            async fn list_products() -> &'static str {
                "products"
            }
        };

        let output = route_macro_core("GET", path, input);
        let output_str = output.to_string();

        assert!(!output_str.contains("x-rapina-cache-ttl"));
    }

    #[test]
    fn test_cache_attr_with_extractors() {
        let path = quote!("/users/:id");
        let input = quote! {
            #[cache(ttl = 120)]
            async fn get_user(id: rapina::extract::Path<u64>) -> String {
                format!("{}", id.into_inner())
            }
        };

        let output = route_macro_core("GET", path, input);
        let output_str = output.to_string();

        assert!(output_str.contains("x-rapina-cache-ttl"));
        assert!(output_str.contains("120"));
        // Single arg uses FromRequest (positional convention)
        assert!(output_str.contains("FromRequest"));
    }

    #[test]
    fn test_group_param_joins_path() {
        let attr = quote!("/users", group = "/api");
        let input = quote! {
            async fn list_users() -> &'static str {
                "users"
            }
        };

        let output = route_macro_core("GET", attr, input);
        let output_str = output.to_string();

        assert!(output_str.contains("path : \"/api/users\""));
        assert!(output_str.contains("__rapina_router . get (\"/api/users\""));
    }

    #[test]
    fn test_group_param_with_nested_prefix() {
        let attr = quote!("/items", group = "/api/v1");
        let input = quote! {
            async fn list_items() -> &'static str {
                "items"
            }
        };

        let output = route_macro_core("GET", attr, input);
        let output_str = output.to_string();

        assert!(output_str.contains("path : \"/api/v1/items\""));
    }

    #[test]
    fn test_without_group_param_backward_compatible() {
        let attr = quote!("/users");
        let input = quote! {
            async fn list_users() -> &'static str {
                "users"
            }
        };

        let output = route_macro_core("GET", attr, input);
        let output_str = output.to_string();

        assert!(output_str.contains("path : \"/users\""));
        assert!(output_str.contains("__rapina_router . get (\"/users\""));
    }

    #[test]
    #[should_panic(expected = "group prefix must start with `/`")]
    fn test_group_prefix_must_start_with_slash() {
        let attr = quote!("/users", group = "api");
        let input = quote! {
            async fn list_users() -> &'static str {
                "users"
            }
        };

        route_macro_core("GET", attr, input);
    }

    #[test]
    fn test_group_with_trailing_slash_normalized() {
        let attr = quote!("/users", group = "/api/");
        let input = quote! {
            async fn list_users() -> &'static str {
                "users"
            }
        };

        let output = route_macro_core("GET", attr, input);
        let output_str = output.to_string();

        assert!(output_str.contains("path : \"/api/users\""));
    }

    #[test]
    fn test_group_with_public_attr() {
        let attr = quote!("/health", group = "/api");
        let input = quote! {
            #[public]
            async fn health() -> &'static str {
                "ok"
            }
        };

        let output = route_macro_core("GET", attr, input);
        let output_str = output.to_string();

        assert!(output_str.contains("path : \"/api/health\""));
        assert!(output_str.contains("is_public : true"));
    }

    #[test]
    fn test_group_with_cache_attr() {
        let attr = quote!("/products", group = "/api");
        let input = quote! {
            #[cache(ttl = 60)]
            async fn list_products() -> &'static str {
                "products"
            }
        };

        let output = route_macro_core("GET", attr, input);
        let output_str = output.to_string();

        assert!(output_str.contains("path : \"/api/products\""));
        assert!(output_str.contains("x-rapina-cache-ttl"));
        assert!(output_str.contains("60"));
    }

    #[test]
    fn test_group_with_errors_attr() {
        let attr = quote!("/users", group = "/api");
        let input = quote! {
            #[errors(UserError)]
            async fn get_user() -> Result<Json<UserResponse>> {
                Ok(Json(UserResponse { id: 1 }))
            }
        };

        let output = route_macro_core("GET", attr, input);
        let output_str = output.to_string();

        assert!(output_str.contains("path : \"/api/users\""));
        assert!(output_str.contains("fn error_responses"));
        assert!(output_str.contains("UserError"));
    }

    #[test]
    fn test_group_with_all_methods() {
        for method in &["GET", "POST", "PUT", "DELETE"] {
            let attr = quote!("/items", group = "/api");
            let input = quote! {
                async fn handler() -> &'static str {
                    "ok"
                }
            };

            let output = route_macro_core(method, attr, input);
            let output_str = output.to_string();

            assert!(
                output_str.contains("path : \"/api/items\""),
                "{method} should produce /api/items"
            );
            let method_lower = method.to_lowercase();
            assert!(
                output_str.contains(&format!("__rapina_router . {method_lower}")),
                "{method} should use .{method_lower}() on router"
            );
        }
    }

    #[test]
    fn test_join_paths_basic() {
        assert_eq!(join_paths("/api", "/users"), "/api/users");
        assert_eq!(join_paths("/api/v1", "/items"), "/api/v1/items");
    }

    #[test]
    fn test_join_paths_trailing_slash() {
        assert_eq!(join_paths("/api/", "/users"), "/api/users");
    }

    #[test]
    fn test_join_paths_empty_path() {
        assert_eq!(join_paths("/api", ""), "/api");
        assert_eq!(join_paths("/api", "/"), "/api");
    }

    #[test]
    fn test_join_paths_empty_prefix() {
        assert_eq!(join_paths("", "/users"), "/users");
        assert_eq!(join_paths("", ""), "/");
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

    // -- new OpenAPI macro attributes --

    #[test]
    fn test_id_attr_generates_operation_id_impl() {
        let path = quote!("/users", id = "users.list");
        let input = quote! {
            async fn list_users() -> &'static str { "users" }
        };
        let output = route_macro_core("GET", path, input).to_string();
        assert!(output.contains("fn operation_id"));
        assert!(output.contains("\"users.list\""));
    }

    #[test]
    fn test_summary_attr_generates_summary_impl() {
        let path = quote!("/users", summary = "List all users");
        let input = quote! {
            async fn list_users() -> &'static str { "users" }
        };
        let output = route_macro_core("GET", path, input).to_string();
        assert!(output.contains("fn summary"));
        assert!(output.contains("\"List all users\""));
    }

    #[test]
    fn test_tags_attr_generates_tags_impl() {
        let path = quote!("/users", tags = ["users", "admin"]);
        let input = quote! {
            async fn list_users() -> &'static str { "users" }
        };
        let output = route_macro_core("GET", path, input).to_string();
        assert!(output.contains("fn tags"));
        assert!(output.contains("\"users\""));
        assert!(output.contains("\"admin\""));
    }

    #[test]
    fn test_deprecated_attr_generates_deprecated_impl() {
        let path = quote!("/old-endpoint", deprecated = true);
        let input = quote! {
            async fn old_handler() -> &'static str { "old" }
        };
        let output = route_macro_core("GET", path, input).to_string();
        assert!(output.contains("fn deprecated"));
        assert!(output.contains("true"));
    }

    #[test]
    fn test_all_new_attrs_combined() {
        let path = quote!(
            "/users",
            id = "users.create",
            summary = "Create a user",
            tags = ["users"],
            deprecated = false
        );
        let input = quote! {
            async fn create_user() -> &'static str { "created" }
        };
        let output = route_macro_core("POST", path, input).to_string();
        assert!(output.contains("\"users.create\""));
        assert!(output.contains("\"Create a user\""));
        assert!(output.contains("\"users\""));
        // deprecated = false should NOT emit the deprecated() method
        assert!(!output.contains("fn deprecated"));
    }

    #[test]
    fn test_no_new_attrs_no_optional_impls() {
        let path = quote!("/users");
        let input = quote! {
            async fn list_users() -> &'static str { "users" }
        };
        let output = route_macro_core("GET", path, input).to_string();
        assert!(!output.contains("fn operation_id"));
        assert!(!output.contains("fn summary"));
        assert!(!output.contains("fn tags"));
        assert!(!output.contains("fn deprecated"));
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
