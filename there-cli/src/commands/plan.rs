use anyhow::{Context, Result};
use async_trait::async_trait;

use super::Interactive;

pub struct PlanCommand;

impl PlanCommand {
    pub fn new() -> Self {
        Self
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
                use tokio::fs::read_to_string;

                let file = self.read_argument_with_validator(matches, "file", &mut |_| Ok(()))?;
                let plan = read_to_string(file).await?;
                let mut task_set: libthere::plan::TaskSet = serde_yaml::from_str(plan.as_str()).context("Failed deserializing plan")?;
                let mut plan = task_set.plan().await?;
                if plan.validate().await.is_ok() {
                    println!("Plan is valid.");
                } else {
                    println!("Plan is invalid.");
                }
            }
            Some((name, _)) => return Err(super::CommandErrors::InvalidSubcommand(name.to_string()).into()),
            None => return Err(super::CommandErrors::NoSubcommandProvided.into()),
        }
        Ok(())
    }
}

impl<'a> super::Interactive<'a> for PlanCommand {}
