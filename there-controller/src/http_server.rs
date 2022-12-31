use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{Json, Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
use color_eyre::eyre::Result;
use nanoid::nanoid;
use quick_cache::sync::Cache;
use serde::{Deserialize, Serialize};
use there::ipc::http::{JobStartRequest, JobState, JobStatus};
use there::log::*;
use thrussh_keys::PublicKeyBase64;
use tokio::sync::Mutex;

#[derive(Debug)]
pub struct ServerState {
    pub jobs: Cache<String, JobState>,
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
        .route("/api/plan/:job_id/status", get(get_job_status))
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
        status: JobStatus::Running,
    };

    let job_accessible_state = state.clone();
    let state = state.lock().await;
    state.jobs.insert(job_id.clone(), job_state);

    let job_id_clone = job_id.clone();
    tokio::spawn(async move {
        match crate::executor::apply_plan(
            &job_id_clone,
            &job_accessible_state,
            job_accessible_body.plan,
            job_accessible_body.hosts,
        )
        .await
        {
            Ok(_) => {
                let state = job_accessible_state.lock().await;
                if let Some(mut job_state) = state.jobs.get(&job_id_clone.clone()) {
                    job_state.status = JobStatus::Completed;
                    state.jobs.insert(job_id_clone.clone(), job_state);
                } else {
                    error!("job {job_id_clone} disappeared from state");
                }
            }
            Err(e) => {
                error!("error applying plan {job_id_clone}: {e}");
                let state = job_accessible_state.lock().await;
                if let Some(mut job_state) = state.jobs.get(&job_id_clone.clone()) {
                    job_state.status = JobStatus::Failed;
                    state.jobs.insert(job_id_clone.clone(), job_state);
                } else {
                    error!("job {job_id_clone} disappeared from state");
                }
            }
        }
    });

    (StatusCode::OK, job_id)
}

#[tracing::instrument]
async fn get_job_status(
    State(state): State<Arc<Mutex<ServerState>>>,
    Path(job_id): Path<String>,
) -> impl IntoResponse {
    let state = state.lock().await;
    let job_state = state.jobs.get(&job_id).unwrap();

    (StatusCode::OK, serde_json::to_string(&job_state).unwrap())
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

        (
            StatusCode::OK,
            format!("ssh-ed25519 {} there-controller", pubkey),
        )
    } else {
        (StatusCode::UNAUTHORIZED, "invalid token".into())
    }
}
