//! Swagger UI endpoint for interactive API exploration.
//!
//! Enabled by the `swagger-ui` feature. Serves a self-contained HTML page that
//! loads the Swagger UI bundle from a CDN and points it at the OpenAPI JSON
//! endpoint (`/__rapina/openapi.json`).
//!
//! # Usage
//!
//! ```rust,ignore
//! Rapina::new()
//!     .openapi("My API", "1.0.0")
//!     .enable_swagger_ui()                    // default path: /__rapina/swagger
//!     // or: .with_swagger_ui(SwaggerUiConfig::new("/__rapina/docs"))
//!     .discover()
//!     .listen("127.0.0.1:3000")
//!     .await?;
//! ```

use std::sync::Arc;

use http::{Request, Response, StatusCode, header::CONTENT_TYPE};
use hyper::body::Incoming;

use crate::{
    extract::PathParams,
    response::{BoxBody, full},
    state::AppState,
};

/// Configuration for the Swagger UI endpoint.
///
/// Use [`SwaggerUiConfig::new`] to set a custom path, or pass this struct
/// to [`Rapina::with_swagger_ui`](crate::app::Rapina::with_swagger_ui).
///
/// # Example
///
/// ```rust,no_run
/// use rapina::prelude::*;
/// use rapina::openapi::SwaggerUiConfig;
///
/// # #[tokio::main]
/// # async fn main() -> std::io::Result<()> {
/// Rapina::new()
///     .openapi("My API", "1.0.0")
///     .with_swagger_ui(SwaggerUiConfig::new("/docs"))
///     .listen("127.0.0.1:3000")
///     .await
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct SwaggerUiConfig {
    /// Path where Swagger UI is served (e.g. `/__rapina/swagger`).
    pub path: String,
    /// URL of the OpenAPI JSON spec. Defaults to `/__rapina/openapi.json`.
    pub spec_url: String,
}

impl SwaggerUiConfig {
    /// Creates a new config serving Swagger UI at `path`, pointing at the
    /// default OpenAPI spec URL (`/__rapina/openapi.json`).
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            spec_url: "/__rapina/openapi.json".to_string(),
        }
    }

    /// Overrides the OpenAPI spec URL (e.g. when the spec is hosted externally).
    pub fn with_spec_url(mut self, spec_url: impl Into<String>) -> Self {
        self.spec_url = spec_url.into();
        self
    }
}

const SWAGGER_UI_VERSION: &str = "5.18.2";

/// Generates the Swagger UI HTML page.
fn swagger_ui_html(spec_url: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Swagger UI</title>
  <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@{SWAGGER_UI_VERSION}/swagger-ui.css">
  <style>
    body {{ margin: 0; }}
    #swagger-ui {{ max-width: 1460px; margin: 0 auto; }}
  </style>
</head>
<body>
  <div id="swagger-ui"></div>
  <script src="https://unpkg.com/swagger-ui-dist@{SWAGGER_UI_VERSION}/swagger-ui-bundle.js"></script>
  <script src="https://unpkg.com/swagger-ui-dist@{SWAGGER_UI_VERSION}/swagger-ui-standalone-preset.js"></script>
  <script>
    window.onload = function() {{
      SwaggerUIBundle({{
        url: "{spec_url}",
        dom_id: '#swagger-ui',
        presets: [
          SwaggerUIBundle.presets.apis,
          SwaggerUIStandalonePreset
        ],
        layout: "StandaloneLayout",
        deepLinking: true,
        showExtensions: true,
        showCommonExtensions: true
      }});
    }};
  </script>
</body>
</html>
"#
    )
}

/// Handler that serves the Swagger UI HTML page.
pub async fn swagger_ui_handler(
    _req: Request<Incoming>,
    _params: PathParams,
    state: Arc<AppState>,
) -> Response<BoxBody> {
    let config = state.get::<SwaggerUiConfig>();

    match config {
        Some(config) => {
            let html = swagger_ui_html(&config.spec_url);
            Response::builder()
                .status(StatusCode::OK)
                .header(CONTENT_TYPE, "text/html; charset=utf-8")
                .body(full(bytes::Bytes::from(html)))
                .unwrap()
        }
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(CONTENT_TYPE, "text/plain")
            .body(full(bytes::Bytes::from_static(b"Swagger UI not configured")))
            .unwrap(),
    }
}
