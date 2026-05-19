use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

use crate::{auth::api_keys, db::queries};

pub async fn auth(
    State(db): State<Arc<Mutex<Connection>>>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let token = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let hash = api_keys::hash_key(&token);

    let ctx = {
        let conn = db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        queries::validate_api_key(&conn, &hash)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    };

    let ctx = ctx.ok_or(StatusCode::UNAUTHORIZED)?;

    req.extensions_mut().insert(ctx);
    Ok(next.run(req).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        middleware,
        routing::get,
        Router,
    };
    use tower::util::ServiceExt;

    use crate::db::{connection::connect, migrations};

    fn make_db() -> Arc<Mutex<Connection>> {
        let conn = connect(":memory:").unwrap();
        migrations::run(&conn).unwrap();
        Arc::new(Mutex::new(conn))
    }

    fn app(db: Arc<Mutex<Connection>>) -> Router {
        Router::new()
            .route("/protected", get(|| async { "ok" }))
            .layer(middleware::from_fn_with_state(db.clone(), auth))
            .with_state(db)
    }

    #[tokio::test]
    async fn missing_auth_header_returns_401() {
        let db = make_db();
        let response = app(db)
            .oneshot(
                Request::builder()
                    .uri("/protected")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn invalid_key_returns_401() {
        let db = make_db();
        let response = app(db)
            .oneshot(
                Request::builder()
                    .uri("/protected")
                    .header("Authorization", "Bearer nm_invalid_key_that_does_not_exist")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
