use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{Json, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
use color_eyre::eyre::Result;
use libthere::ipc::http::JobStartRequest;
use libthere::plan::host::HostConfig;
use libthere::plan::Plan;
use nanoid::nanoid;
use quick_cache::sync::Cache;
use serde::{Deserialize, Serialize};
use thrussh_keys::PublicKeyBase64;
use tokio::sync::Mutex;

use crate::executor::LogEntry;

#[derive(Debug)]
pub struct ServerState {
    pub jobs: Cache<String, JobState>,
}

#[derive(Clone, Debug)]
pub struct JobState {
    pub logs: HashMap<String, Vec<LogEntry>>,
    pub plan: Plan,
    pub hosts: HostConfig,
}

#[tracing::instrument]
pub async fn run_server(port: u16) -> Result<()> {
    let state = Arc::new(Mutex::new(ServerState {
        jobs: Cache::new(1000),
    }));

    let app = Router::new()
        .route("/", get(root))
        .route("/api/bootstrap", get(bootstrap))
        .route("/api/plan/run", post(run_plan))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .map_err(|e| e.into())
}

#[tracing::instrument]
async fn root() -> &'static str {
    "there-controller"
}

#[tracing::instrument]
async fn run_plan(
    State(state): State<Arc<Mutex<ServerState>>>,
    Json(body): Json<JobStartRequest>,
) -> impl IntoResponse {
    let job_id = nanoid!(21, nanoid_dictionary::NOLOOKALIKES_SAFE);
    let job_accessible_body = body.clone();
    let job_state = JobState {
        logs: HashMap::new(),
        plan: body.plan,
        hosts: body.hosts,
    };

    let job_accessible_state = state.clone();
    let state = state.lock().await;
    state.jobs.insert(job_id.clone(), job_state);

    let job_id_clone = job_id.clone();
    tokio::spawn(async move {
        crate::executor::apply_plan(
            job_id_clone,
            job_accessible_state,
            job_accessible_body.plan,
            job_accessible_body.hosts,
        )
        .await
        .unwrap();
    });

    (StatusCode::OK, job_id)
}

#[derive(Serialize, Deserialize, Debug)]
struct Bootstrap {
    token: String,
}

#[tracing::instrument]
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
