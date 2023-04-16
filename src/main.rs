//! Test whether the CORS (Access-Control-Allow-Origin) header is set correctly
//! even when the response fails.

use axum::extract::Path;
use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use thiserror::Error;
use tracing::{error, info, warn};

/// An error type that implements axum::IntoResponse.
/// This allows any HTTP errors to be directly thrown by the handler.
#[derive(Debug, Error)]
enum AppError {
    #[error("Not Found")]
    NotFound(#[source] anyhow::Error),
    #[error("Internal Server Error")]
    InternalServerError(
        #[from]
        #[source]
        anyhow::Error,
    ),
}

// Allows being returned by a handler.
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::NotFound(_e) => (StatusCode::NOT_FOUND, "Not Found").into_response(),
            AppError::InternalServerError(_e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response()
            }
        }
    }
}

/// A handler that returns "200 OK" when the path is "200,"
/// "500 Internal Server Error" when the path is "500,"
/// and "404 Not Found" for any other path or exceptional case.
///
/// I'd like to be able to return a CORS header even when the response fails.
async fn handler(path: Option<Path<String>>) -> Result<(StatusCode, &'static str), AppError> {
    if path.is_none() {
        return Ok((
            StatusCode::BAD_REQUEST,
            "usage: /200 or /500.\r\nCheck for CORS header.\r\n",
        ));
    }

    let Path(path) = path.unwrap();
    match path.as_str() {
        "200" => Ok((StatusCode::OK, "200 OK\r\n")),
        "500" => Err(AppError::InternalServerError(anyhow::anyhow!(
            "500 Internal Server Error\r\n"
        ))),
        _ => Err(AppError::NotFound(anyhow::anyhow!("404 Not Found\r\n"))),
    }
}

#[tokio::main]
async fn main() -> Result<(), &'static str> {
    // Initialize tracing.
    tracing_subscriber::fmt::init();

    // If the environment variable RUST_LOG isn't set, let the user know
    // how to access the HTTP logs by setting it to DEBUG.
    let rust_log = std::env::var("RUST_LOG");
    match rust_log {
        Ok(rust_log) => {
            // If set, let them know.
            info!("RUST_LOG={}", rust_log);
        }
        Err(e) => {
            // Discern which kind.
            // If not set, warn.
            // Any other error: let them know.
            match e {
                std::env::VarError::NotPresent => {
                    warn!("Set RUST_LOG=DEBUG to see HTTP logs.");
                }
                _ => {
                    error!("Error getting RUST_LOG: {}", e);
                    return Err("Bad Env");
                }
            }
        }
    };

    // Check for optional argument at argv[1].
    let port = match std::env::args().nth(1) {
        Some(port) => port,
        None => {
            warn!("Port not set. Defaulting to 3000. (Set as first argument.)");
            "3000".to_string()
        }
    };

    // CORS Layer. Allow listening on any origin, GET requests.
    let cors = tower_http::cors::CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods([Method::GET]);

    // Create a new router.
    let app = axum::Router::new()
        // Add a route that matches any request to "/:code" and calls the handler.
        // Actually, match everything.
        .route("/:code", get(handler))
        .route("/", get(handler))
        // Note: fallback is not used to enable testing unhandled routing.
        // Add a CORS middleware.
        .layer(cors)
        // Add tracing logging.
        .layer(tower_http::trace::TraceLayer::new_for_http());

    // Let's go
    let str_bind_to = format!("0.0.0.0:{port}");
    let bind_to = str_bind_to.parse();
    if let Err(ref e) = bind_to {
        error!("Error parsing {str_bind_to:?} as socket address: {e}");
        return Err("Bad Bind");
    }
    let bind_to = bind_to.unwrap();
    info!("Listening on {}", bind_to);
    let future = axum::Server::bind(&bind_to)
        .serve(app.into_make_service())
        .await;
    if let Err(ref e) = future {
        error!("Failure to launch: {e}");
        return Err("Bad Launch");
    }

    Ok(())
}
