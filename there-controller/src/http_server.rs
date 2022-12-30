use std::net::SocketAddr;

use axum::extract::Query;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use color_eyre::eyre::Result;
use serde::{Deserialize, Serialize};
use thrussh_keys::PublicKeyBase64;

pub async fn run_server(port: u16) -> Result<()> {
    let app = Router::new()
        .route("/", get(root))
        .route("/api/bootstrap", get(bootstrap));

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .map_err(|e| e.into())
}

async fn root() -> &'static str {
    "there-controller"
}

#[derive(Serialize, Deserialize)]
struct Bootstrap {
    token: String,
}

async fn bootstrap(bootstrap: Query<Bootstrap>) -> impl IntoResponse {
    let token = crate::keys::get_or_create_token().await.unwrap();
    if token == bootstrap.token {
        let pubkey = crate::keys::get_or_create_executor_keypair()
            .await
            .map(|keys| keys.public_key_base64())
            .unwrap();

        (StatusCode::OK, pubkey)
    } else {
        (StatusCode::UNAUTHORIZED, "invalid token".into())
    }
}
