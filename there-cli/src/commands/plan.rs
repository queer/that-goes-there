use super::Interactive;
use anyhow::{Context, Result};
use async_trait::async_trait;
use clap::ArgMatches;
use libthere::executor::{simple, Executor, LogSource, PartialLogStream};
use libthere::{log::*, plan};
use tokio::fs::{self, read_to_string};
use tokio::sync::{mpsc, Mutex};

use crate::executor::ssh;

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
    async fn subcommand_validate<'a>(
        &self,
        _context: &'a super::CliContext<'a>,
        matches: &ArgMatches,
    ) -> Result<()> {
        let file = self.read_argument_with_validator(matches, "file", &mut |_| Ok(()))?;
        let plan = read_to_string(file).await?;
        let mut task_set: libthere::plan::TaskSet =
            serde_yaml::from_str(plan.as_str()).context("Failed deserializing plan")?;
        let mut plan = task_set.plan().await?;
        if plan.validate().await.is_ok() {
            info!("Plan is valid.");
        } else {
            error!("Plan is invalid.");
        }
        Ok(())
    }

    async fn subcommand_apply<'a>(
        &self,
        _context: &'a super::CliContext<'a>,
        matches: &ArgMatches,
    ) -> Result<()> {
        let file = self.read_argument_with_validator(matches, "file", &mut |_| Ok(()))?;
        let plan = read_to_string(file).await?;
        let mut task_set: libthere::plan::TaskSet =
            serde_yaml::from_str(plan.as_str()).context("Failed deserializing plan")?;
        let mut plan = task_set.plan().await?;
        let (plan, validation_errors) = plan.validate().await?;

        if validation_errors.is_empty() {
            if *matches.get_one::<bool>("dry").unwrap() {
                println!("*** plan: {} ***\n", plan.name());
                for task in plan.blueprint() {
                    println!("* {}: {}", task.name(), task.command().join(" "));
                }
            } else {
                info!("applying plan...");
                let executor_type: ExecutorType =
                    match matches.get_one::<String>("executor").unwrap().as_str() {
                        "local" => ExecutorType::Local,
                        "ssh" => ExecutorType::Ssh,
                        _ => unreachable!(),
                    };
                self.do_apply(plan, executor_type, matches).await?;
                info!("done!");
            }
        } else {
            error!("Plan is invalid!");
            for error in validation_errors {
                error!("- {}", error);
            }
        }

        Ok(())
    }

    async fn do_apply(
        &self,
        plan: plan::Plan,
        executor_type: ExecutorType,
        matches: &ArgMatches,
    ) -> Result<()> {
        let (tx, rx) = mpsc::channel(1024);
        let mut log_source = libthere::executor::simple::SimpleLogSource::new(rx);
        let join_handle = tokio::task::spawn(async move {
            'outer: while let Ok(partial_stream) = log_source.source().await {
                match partial_stream {
                    PartialLogStream::Next(logs) => {
                        for log in logs {
                            println!("{}", log);
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
        match executor_type {
            ExecutorType::Local => {
                let mut context = simple::SimpleExecutionContext::new("test", plan);
                let context = Mutex::new(&mut context);
                let executor = simple::SimpleExecutor::new(&tx);
                executor.execute(context).await?;
            }
            ExecutorType::Ssh => {
                let ssh_key_file = matches
                    .get_one::<String>("ssh-key")
                    .context("--ssh-key wasn't passed")?;
                let ssh_key = fs::read_to_string(ssh_key_file)
                    .await
                    .context("Failed reading ssh key file")?;

                let ssh_user = matches
                    .get_one::<String>("ssh-user")
                    .context("--ssh-user wasn't passed")?;
                let ssh_key_passphrase = matches.get_one::<String>("ssh-key-passphrase").map(|s| {
                    std::fs::read_to_string(s).context("Failed to read ssh key passphrase")
                });
                let ssh_key_passphrase = match ssh_key_passphrase {
                    Some(Ok(passphrase)) => Some(passphrase),
                    Some(Err(e)) => return Err(e),
                    None => None,
                };
                let mut context = ssh::SshExecutionContext::new("test", plan);
                let context = Mutex::new(&mut context);
                let executor =
                    ssh::SshExecutor::new(&tx, ssh_user.clone(), ssh_key, ssh_key_passphrase);
                executor.execute(context).await?;
            }
            #[allow(unreachable_patterns)]
            _ => {
                unreachable!()
            }
        };
        info!("finished applying plan");
        join_handle.await?;
        Ok(())
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
