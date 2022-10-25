use super::Interactive;
use anyhow::{Context, Result};
use async_trait::async_trait;
use clap::ArgMatches;
use libthere::executor::{Executor, LogSource, self};
use libthere::{log::*, plan};
use tokio::fs::read_to_string;
use tokio::sync::{mpsc, Mutex};

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
            info!("applying plan...");
            self.do_apply(plan, &()).await?;
            info!("done!");
        } else {
            error!("Plan is invalid!");
            for error in validation_errors {
                error!("- {}", error);
            }
        }

        Ok(())
    }

    async fn do_apply<'a>(&self, plan: plan::Plan, _lifetime_eater: &'a ()) -> Result<()> {
        let (tx, rx) = mpsc::channel(1024);
        let mut log_source = libthere::executor::simple::SimpleLogSource::new(rx);
        let join_handle = tokio::task::spawn(async move {
            'outer: while let Ok(logs) = log_source.source().await {
                for log in logs {
                    if log == executor::simple::MAGIC_MESSAGE_THAT_KILLS_THE_TX {
                        break 'outer;
                    }
                    println!("{}", log);
                }
            }
            info!("join finished :D");
        });

        let executor = libthere::executor::simple::SimpleExecutor::new(&tx);
        let mut context =
            libthere::executor::simple::SimpleExecutionContext::new("test", plan);
        executor.execute(Mutex::new(&mut context)).await?;
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
