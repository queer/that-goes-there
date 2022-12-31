use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use super::Interactive;
use async_trait::async_trait;
use clap::ArgMatches;
use color_eyre::eyre::Result;
use futures::stream::FuturesUnordered;
use libthere::executor::{simple, ssh, Executor, LogSource, PartialLogStream};
use libthere::ipc::http::{JobState, JobStatus};
use libthere::plan::host::{Host, HostConfig};
use libthere::{log::*, plan};
use tokio::fs;
use tokio::sync::{mpsc, Mutex};
use tokio_stream::StreamExt;

#[derive(Clone, Debug)]
pub enum ExecutorType {
    Local,
    Ssh,
}

pub struct PlanCommand;

impl PlanCommand {
    pub fn new() -> Self {
        Self
    }
}

impl PlanCommand {
    #[tracing::instrument(skip(self))]
    async fn read_hosts_config(&self, path: &Path) -> Result<HostConfig> {
        let hosts = fs::read_to_string(path).await?;
        serde_yaml::from_str(hosts.as_str()).map_err(color_eyre::eyre::Report::new)
    }

    #[tracing::instrument(skip(self, _context))]
    async fn subcommand_validate<'a>(
        &self,
        _context: &'a super::CliContext<'a>,
        matches: &ArgMatches,
    ) -> Result<()> {
        let file = self.read_argument_with_validator(matches, "file", &mut |_| Ok(()))?;
        let hosts_file = &self.read_argument_with_validator(matches, "hosts", &mut |_| Ok(()))?;
        let hosts_file = Path::new(hosts_file);
        let hosts = self.read_hosts_config(hosts_file).await?;

        let plan = fs::read_to_string(file).await?;
        let task_set: libthere::plan::TaskSet = serde_yaml::from_str(plan.as_str())?;
        task_set.plan().await?;
        info!("plan is valid.");
        println!("* plan is valid.");
        println!("** hosts:");
        for (group_name, group_hosts) in hosts.groups() {
            self.inspect_host_group(hosts.hosts(), group_name, group_hosts)?;
        }
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    fn inspect_host_group(
        &self,
        hosts: &HashMap<String, Host>,
        group_name: &String,
        group_hosts: &Vec<String>,
    ) -> Result<()> {
        println!("*** group: {}", group_name);
        for hostname in group_hosts {
            let host = hosts
                .get(hostname)
                .ok_or_else(|| eyre!("host not found: {}", hostname))?;
            println!(
                "**** {}: {}:{} ({})",
                hostname,
                host.host(),
                host.port(),
                host.executor()
            );
        }
        Ok(())
    }

    #[tracing::instrument(skip(self, _context))]
    async fn subcommand_apply<'a>(
        &self,
        _context: &'a super::CliContext<'a>,
        matches: &ArgMatches,
    ) -> Result<()> {
        let file = self.read_argument_with_validator(matches, "file", &mut |_| Ok(()))?;
        let plan = fs::read_to_string(file).await?;
        let task_set: libthere::plan::TaskSet = serde_yaml::from_str(plan.as_str())?;
        let hosts_file = self.read_argument_with_validator(matches, "hosts", &mut |_| Ok(()))?;
        let hosts_file = Path::new(&hosts_file);
        let hosts = self.read_hosts_config(hosts_file).await?;

        let plan = task_set.plan().await?;
        let controller = matches.get_one::<String>("controller");

        if *matches.get_one::<bool>("dry").unwrap() {
            println!("*** plan: {} ***\n", plan.name());
            println!("* metadata");
            println!("** hosts:");
            for (group_name, group_hosts) in hosts.groups() {
                self.inspect_host_group(hosts.hosts(), group_name, group_hosts)?;
            }
            for task in plan.blueprint() {
                println!("** {}: {}", task.name(), task.command().join(" "));
                for ensure in task.ensures() {
                    println!("*** {:?}", ensure);
                }
            }
        } else {
            info!("applying plan...");
            let mut futures = FuturesUnordered::new();
            if let Some(controller) = controller {
                info!("applying plan via controller {controller}!");
                // TODO: reqwest::post(controller).json({plan, hosts}).await?; stream_logs();
                let client = reqwest::Client::new();
                let res = client
                    .post(format!("{controller}/api/plan/run"))
                    .json(&serde_json::json!({ "plan": plan, "hosts": hosts }))
                    .send()
                    .await?;
                let job_id = res.text().await?;
                debug!("job_id: {job_id}");
                println!("* plan assigned controller job id: {job_id}");

                let mut log_offsets = HashMap::new();
                loop {
                    let job_state = client
                        .get(format!("{controller}/api/plan/{job_id}/status"))
                        .send()
                        .await?;
                    let job_state = job_state.json::<JobState>().await?;

                    for (hostname, logs) in job_state.logs {
                        if logs.len() > *log_offsets.get(&hostname).unwrap_or(&0usize) {
                            for log in logs
                                .iter()
                                .skip(*log_offsets.get(&hostname).unwrap_or(&0usize))
                            {
                                println!("{}: {}", log.hostname, log.log);
                            }
                            log_offsets.insert(hostname, logs.len());
                        }
                    }

                    if job_state.status == JobStatus::Completed {
                        println!("* plan completed!");
                        info!("finished execution for plan {job_id}");
                        break;
                    }

                    if job_state.status == JobStatus::Failed {
                        println!("* plan execution failed!");
                        info!("failed execution for plan {job_id}");
                        break;
                    }

                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            } else {
                info!("applying plan via executor!");
                for (group_name, group_hosts) in hosts.groups() {
                    println!("*** applying plan to group: {} ***", group_name);
                    for hostname in group_hosts {
                        let plan = plan.plan_for_host(hostname, &hosts);
                        if !plan.blueprint().is_empty() {
                            let host = &hosts
                                .hosts()
                                .get(hostname)
                                .ok_or_else(|| eyre!("hostname not in host map: {}", hostname))?;
                            let executor = host.executor();
                            let executor_type: ExecutorType = match executor.as_str() {
                                "simple" => ExecutorType::Local,
                                "local" => ExecutorType::Local,
                                "ssh" => ExecutorType::Ssh,
                                _ => return Err(eyre!("unknown executor type: {}", executor)),
                            };
                            futures.push(self.do_apply(
                                plan,
                                hostname.clone(),
                                host,
                                executor_type,
                                matches,
                            ));
                            println!("*** prepared plan for host: {}", &hostname);
                        } else {
                            println!("*** skipping host, no tasks: {}", &hostname);
                        }
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
            info!("done!");
        }

        Ok(())
    }

    /// Returns how many tasks passed.
    #[tracing::instrument(skip(self, plan, matches))]
    async fn do_apply(
        &self,
        plan: plan::Plan,
        hostname: String,
        host: &Host,
        executor_type: ExecutorType,
        matches: &ArgMatches,
    ) -> Result<(String, u32)> {
        let (tx, rx) = mpsc::channel(1024);
        let mut log_source = libthere::executor::simple::SimpleLogSource::new(rx);
        let log_hostname = hostname.clone();
        let ssh_hostname = hostname.clone();
        let join_handle = tokio::task::spawn(async move {
            'outer: while let Ok(partial_stream) = log_source.source().await {
                match partial_stream {
                    PartialLogStream::Next(logs) => {
                        for log in logs {
                            println!("{}: {}", log_hostname, log);
                        }
                    }
                    PartialLogStream::End => {
                        break 'outer;
                    }
                }
            }
            info!("join finished :D");
        });

        // TODO: Figure out this generics mess lmao
        let tasks_completed = match executor_type {
            ExecutorType::Local => {
                let mut context = simple::SimpleExecutionContext::new("test", &plan);
                let context = Mutex::new(&mut context);
                let mut executor = simple::SimpleExecutor::new(&tx);
                executor.execute(context).await
                .map_err(|err| {
                    eyre!(
                        "local executor failed to apply plan {} to host {}: {}/{} tasks finished:\n\n{:?}",
                        plan.name(),
                        hostname,
                        executor.tasks_completed(),
                        plan.blueprint().len(),
                        err
                    )
                })
                ?;
                Ok(*executor.tasks_completed())
            }
            ExecutorType::Ssh => {
                let ssh_key_file = matches
                    .get_one::<String>("ssh-key")
                    .ok_or(eyre!("--ssh-key not passed!"))?;
                let ssh_key = fs::read_to_string(ssh_key_file).await?;

                let ssh_key_passphrase = matches
                    .get_one::<String>("ssh-key-passphrase")
                    .map(std::fs::read_to_string);
                let ssh_key_passphrase = match ssh_key_passphrase {
                    Some(Ok(passphrase)) => Some(passphrase),
                    Some(Err(e)) => {
                        return Err(eyre!("failed to read ssh-key-passphrase file: {}", e))
                    }
                    None => None,
                };
                let mut context = ssh::SshExecutionContext::new("test", &plan);
                let context = Mutex::new(&mut context);
                #[allow(clippy::or_fun_call)]
                let mut executor =
                    ssh::SshExecutor::new(host, &ssh_hostname, &tx, &ssh_key, ssh_key_passphrase)?;
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
            }
            #[allow(unreachable_patterns)]
            _ => {
                unreachable!()
            }
        };
        info!("finished applying plan");
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
}

#[async_trait]
impl<'a> super::Command<'a> for PlanCommand {
    fn new() -> Self
    where
        Self: Sized,
    {
        Self {}
    }

    #[tracing::instrument(skip(self, context))]
    async fn run(&self, context: &'a super::CliContext) -> Result<()> {
        match context.matches.subcommand() {
            Some(("validate", matches)) => {
                self.subcommand_validate(context, matches).await?;
            }
            Some(("apply", matches)) => {
                self.subcommand_apply(context, matches).await?;
            }
            Some((name, _)) => {
                return Err(super::CommandErrors::InvalidSubcommand(name.to_string()).into())
            }
            None => return Err(super::CommandErrors::NoSubcommandProvided.into()),
        }
        Ok(())
    }
}

impl<'a> super::Interactive<'a> for PlanCommand {}

#[derive(thiserror::Error, Debug)]
enum PlanApplyErrors {
    #[error("failed to apply plan to host: {0} ({1} tasks complete): {2}")]
    PlanApplyFailed(String, u32, color_eyre::eyre::Error),
}
