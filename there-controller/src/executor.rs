use std::sync::Arc;

use color_eyre::eyre::Result;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use there::executor::{ssh, Executor, LogSource, PartialLogStream};
use there::ipc::http::LogEntry;
use there::log::*;
use there::plan::host::{Host, HostConfig};
use there::plan::Plan;
use tokio::sync::{mpsc, Mutex};

use crate::http_server::ServerState;

#[tracing::instrument]
pub async fn apply_plan<'a>(
    job_id: &String,
    server_state: &Arc<Mutex<ServerState>>,
    plan: Plan,
    hosts: HostConfig,
) -> Result<()> {
    let mut futures = FuturesUnordered::new();
    #[allow(clippy::for_kv_map)]
    for (_group_name, group_hosts) in hosts.groups() {
        // println!("*** applying plan to group: {} ***", group_name);
        for hostname in group_hosts {
            let plan = plan.plan_for_host(hostname, &hosts);
            if !plan.blueprint().is_empty() {
                let host = &hosts
                    .hosts()
                    .get(hostname)
                    .ok_or_else(|| eyre!("hostname not in host map: {}", hostname))?;
                futures.push(do_apply(
                    job_id.clone(),
                    plan,
                    hostname.clone(),
                    host,
                    server_state.clone(),
                ));
                // println!("*** prepared plan for host: {}", &hostname);
            } else {
                // println!("*** skipping host, no tasks: {}", &hostname);
            }
        }
    }
    while let Some(result) = futures.next().await {
        match result {
            Ok((host, tasks_completed)) => {
                println!(
                    "*** completed plan: {} for host: {}: {}/{} ***",
                    &plan.name(),
                    host,
                    tasks_completed,
                    plan.blueprint().len()
                );
            }
            Err(e) => {
                warn!("error applying plan: {}", e);
                #[allow(clippy::single_match)]
                match e.downcast() {
                    Ok(PlanApplyErrors::PlanApplyFailed(host, tasks_completed, e)) => {
                        println!(
                            "*** failed plan: {} for host: {}: {}/{} ***",
                            &plan.name(),
                            host,
                            tasks_completed,
                            plan.blueprint().len()
                        );
                        println!("*** error: {:#?}", e);
                    }
                    Err(msg) => {
                        println!("{}", msg);
                    }
                    #[allow(unreachable_patterns)]
                    e => {
                        println!("*** failed plan: ??? for host: ???: ???/??? ***",);
                        println!("*** error: {:#?}", e);
                        println!("THIS SHOULD NEVER HAPPEN");
                    }
                }
            }
        }
    }

    info!("finished applying plan! :D");

    Ok(())
}

/// Returns how many tasks passed.
#[tracing::instrument(skip(plan, host))]
async fn do_apply(
    job_id: String,
    plan: Plan,
    hostname: String,
    host: &Host,
    state: Arc<Mutex<ServerState>>,
) -> Result<(String, u32)> {
    let (tx, rx) = mpsc::channel(1024);
    let mut log_source = there::executor::simple::SimpleLogSource::new(rx);
    let log_hostname = hostname.clone();
    let ssh_hostname = hostname.clone();
    let join_handle = tokio::task::spawn(async move {
        'outer: while let Ok(partial_stream) = log_source.source().await {
            match partial_stream {
                PartialLogStream::Next(logs) => {
                    // Try to hold the server state lock for as little time as
                    // possible.
                    let state = state.lock().await;
                    if let Some(mut job_state) = state.jobs.get(&job_id.clone()) {
                        let mut logs: Vec<LogEntry> = logs
                            .iter()
                            .map(|log| LogEntry {
                                hostname: log_hostname.clone(),
                                log: log.clone(),
                            })
                            .collect();
                        job_state
                            .logs
                            .entry(log_hostname.clone())
                            .or_insert_with(Vec::new);
                        job_state
                            .logs
                            .get_mut(&log_hostname)
                            .unwrap()
                            .append(&mut logs);
                        state.jobs.insert(job_id.clone(), job_state);
                    } else {
                        error!("missing state for job {job_id}!?");
                    }
                }
                PartialLogStream::End => {
                    break 'outer;
                }
            }
        }
    });

    let tasks_completed = {
        let mut context = ssh::SshExecutionContext::new("test", &plan);
        let context = Mutex::new(&mut context);
        #[allow(clippy::or_fun_call)]
        let mut executor = ssh::SshExecutor::new_with_existing_key(
            host,
            &ssh_hostname,
            &tx,
            crate::keys::get_or_create_executor_keypair().await?,
        )?;
        match executor.execute(context).await.map_err(|err| {
            eyre!(
                "ssh executor failed to apply plan {} to host {}: {}/{} tasks finished:\n\n{:?}",
                plan.name(),
                hostname,
                executor.tasks_completed(),
                plan.blueprint().len(),
                err
            )
        }) {
            Ok(_) => Ok(*executor.tasks_completed()),
            Err(e) => Err(PlanApplyErrors::PlanApplyFailed(
                hostname.clone(),
                *executor.tasks_completed(),
                e,
            )),
        }
    };

    match join_handle.await {
        Ok(_) => match tasks_completed {
            Ok(tasks_completed) => Ok((hostname, tasks_completed)),
            Err(e) => Err(eyre!("failed to apply plan: {}", e)),
        },
        e @ Err(_) => e
            .map(|_| (hostname, 0))
            .map_err(color_eyre::eyre::Report::new),
    }
}

#[derive(thiserror::Error, Debug)]
enum PlanApplyErrors {
    #[error("failed to apply plan to host: {0} ({1} tasks complete): {2}")]
    PlanApplyFailed(String, u32, color_eyre::eyre::Error),
}
