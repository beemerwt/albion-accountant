use crate::{
    error::{DecodeError, Result},
    store::{TradeQuery, TradeStore},
    trades::TradeOperation,
};
use axum::{
    Router,
    body::Body,
    extract::{Query, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Deserialize;
use tokio::net::TcpListener;
use tower_http::cors::{Any, CorsLayer};

include!(concat!(env!("OUT_DIR"), "/webapp_assets.rs"));

#[derive(Clone)]
struct AppState {
    store: TradeStore,
}

pub struct WebServer {
    pub url: String,
}

pub async fn start_web_server(store: TradeStore) -> Result<WebServer> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let address = listener.local_addr()?;
    let url = format!("http://{address}");
    let app = router(store);

    tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, app).await {
            eprintln!("ERROR:albion:webserver stopped: {err}");
        }
    });

    eprintln!("INFO:albion:web app listening at {url}");
    Ok(WebServer { url })
}

fn router(store: TradeStore) -> Router {
    Router::new()
        .route("/api/trades", get(api_trades))
        .route("/api/summary", get(api_summary))
        .route("/", get(index))
        .route("/{*path}", get(asset))
        .layer(CorsLayer::new().allow_origin(Any))
        .with_state(AppState { store })
}

#[derive(Debug, Deserialize)]
struct TradeQueryParams {
    page: Option<u32>,
    page_size: Option<u32>,
    q: Option<String>,
    operation: Option<String>,
}

async fn api_trades(
    State(state): State<AppState>,
    Query(params): Query<TradeQueryParams>,
) -> impl IntoResponse {
    let operation = match params
        .operation
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        Some(value) => match TradeOperation::try_from(value) {
            Ok(operation) => Some(operation),
            Err(err) => return (StatusCode::BAD_REQUEST, err).into_response(),
        },
        None => None,
    };
    match state.store.list_trades(TradeQuery {
        page: params.page,
        page_size: params.page_size,
        q: params.q.filter(|value| !value.trim().is_empty()),
        operation,
    }) {
        Ok(list) => axum::Json(list).into_response(),
        Err(err) => server_error(err),
    }
}

async fn api_summary(State(state): State<AppState>) -> impl IntoResponse {
    match state.store.summary() {
        Ok(summary) => axum::Json(summary).into_response(),
        Err(err) => server_error(err),
    }
}

async fn index() -> impl IntoResponse {
    asset_response("/index.html").unwrap_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "embedded web app is missing index.html",
        )
            .into_response()
    })
}

async fn asset(axum::extract::Path(path): axum::extract::Path<String>) -> impl IntoResponse {
    let route = format!("/{path}");
    asset_response(&route)
        .or_else(|| asset_response("/index.html"))
        .unwrap_or_else(|| (StatusCode::NOT_FOUND, "not found").into_response())
}

fn asset_response(route: &str) -> Option<Response> {
    let (_, mime, bytes) = WEBAPP_ASSETS
        .iter()
        .find(|(asset_route, _, _)| *asset_route == route)?;

    let mut response = Response::new(Body::from(*bytes));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(mime).unwrap_or_else(|_| HeaderValue::from_static("text/plain")),
    );
    Some(response)
}

fn server_error(error: DecodeError) -> Response {
    eprintln!("ERROR:albion:web api failed: {}", error.0);
    (StatusCode::INTERNAL_SERVER_ERROR, error.0).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trades::TradeRecord;
    use axum::body::to_bytes;
    use chrono::{Local, TimeZone};
    use serde_json::Value;
    use tower::ServiceExt;

    #[tokio::test]
    async fn trades_api_returns_paginated_rows() {
        let store = test_store();
        store
            .upsert_trade(&trade("1", TradeOperation::Buy))
            .unwrap();
        let app = router(store);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/trades?page=1&page_size=10")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 1);
        assert_eq!(json["items"][0]["id"], "1");
    }

    #[tokio::test]
    async fn summary_api_returns_totals() {
        let store = test_store();
        store
            .upsert_trade(&trade("1", TradeOperation::Buy))
            .unwrap();
        let app = router(store);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/summary")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["row_count"], 1);
    }

    fn test_store() -> TradeStore {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "albion-accountant-web-test-{}-{unique}.sqlite3",
            std::process::id(),
        ));
        let _ = std::fs::remove_file(&path);
        TradeStore::open(&path).unwrap()
    }

    fn trade(id: &str, operation: TradeOperation) -> TradeRecord {
        TradeRecord {
            id: id.to_string(),
            timestamp: Local.with_ymd_and_hms(2026, 5, 29, 12, 0, 0).unwrap(),
            location: "Bridgewatch".to_string(),
            item: "T4_BAG".to_string(),
            operation,
            debit: (operation == TradeOperation::Buy).then_some(100),
            credit: (operation == TradeOperation::Sell).then_some(150),
        }
    }
}
