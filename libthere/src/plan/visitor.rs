use std::path::Path;

use anyhow::{Context, Result};
use async_trait::async_trait;
use derive_getters::Getters;

use super::{Ensure, PlannedTask, Task};
use crate::log::*;

pub trait TaskVisitor: Send + std::fmt::Debug {
    type Out;

    fn visit_task(&mut self, task: &Task) -> Result<Self::Out>;
}

#[derive(Getters, Debug, Clone)]
pub struct PlanningTaskVisitor {
    name: String,
    plan: Vec<PlannedTask>,
}

impl PlanningTaskVisitor {
    pub fn new(name: String) -> Self {
        Self {
            name,
            plan: Vec::new(),
        }
    }
}

impl TaskVisitor for PlanningTaskVisitor {
    type Out = ();

    #[tracing::instrument]
    fn visit_task(&mut self, task: &Task) -> Result<Self::Out> {
        debug!("planning task visitor: visiting task: {}", &task.name());

        match task {
            Task::Command { name, command } => {
                let mut final_command = vec![];
                for shell_word in shell_words::split(command)? {
                    final_command.push(shell_word.clone());
                }
                self.plan.push(PlannedTask::from_shell_command(name, command)?);
            }
            Task::CreateDirectory { name, path } => {
                self.plan.push(PlannedTask {
                    name: name.to_string(),
                    command: vec!["mkdir".into(), path.to_string()],
                    ensures: vec![Ensure::DirectoryExists { path: path.to_string() }],
                });
            }
            Task::TouchFile { name, path } => {
                self.plan.push(PlannedTask {
                    name: name.to_string(),
                    command: vec!["touch".into(), path.to_string()],
                    ensures: vec![Ensure::ExeExists { exe: "touch".into() }],
                });
            }
        }

        debug!(
            "planning task visitor: finished planning task: {}",
            &task.name()
        );
        Ok(())
    }
}

#[async_trait]
pub trait PlannedTaskVisitor: Send {
    type Out;

    async fn visit_planned_task(&mut self, task: &PlannedTask) -> Result<Self::Out>;
}

#[derive(Getters, Debug, Clone, Default)]
pub struct EnsuringTaskVisitor {}

impl EnsuringTaskVisitor {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl PlannedTaskVisitor for EnsuringTaskVisitor {
    type Out = Vec<anyhow::Error>;

    #[tracing::instrument]
    async fn visit_planned_task(&mut self, task: &PlannedTask) -> Result<Self::Out> {
        let mut last_len = 0;
        let mut errors: Vec<anyhow::Error> = vec![];
        for ensure in task.ensures() {
            debug!("ensuring task visitor: checking task: {}", &task.name());

            match ensure {
                Ensure::ExeExists { exe } => {
                    let result = which::which(exe)
                        .with_context(|| format!("{} not found in $PATH", exe))
                        .map(|exe_path| {
                            if !exe_path.is_file() {
                                anyhow::anyhow!(
                                    "Executable '{}' for task '{}' is not a file",
                                    exe,
                                    task.name()
                                )
                            } else {
                                anyhow::anyhow!("")
                            }
                        });
                    if let Err(err) = result {
                        // TODO: Can this hack be avoided?
                        if !format!("{}", err).is_empty() {
                            errors.push(err);
                        }
                    }
                }
                Ensure::DirectoryExists { path } => {
                    let path = Path::new(path);
                    if !path.exists() || !path.is_dir() {
                        errors.push(anyhow::anyhow!(
                            "Directory '{}' for task '{}' does not exist",
                            path.display(),
                            task.name()
                        ));
                    }
                }
                Ensure::FileExists { path } => {
                    let path = Path::new(path);
                    if !path.exists() || !path.is_file() {
                        errors.push(anyhow::anyhow!(
                            "File '{}' for task '{}' does not exist",
                            path.display(),
                            task.name()
                        ));
                    }
                }
                Ensure::DirectoryDoesntExist { path } => {
                    let path = Path::new(path);
                    if path.exists() && path.is_dir() {
                        errors.push(anyhow::anyhow!(
                            "Directory '{}' for task '{}' exists",
                            path.display(),
                            task.name()
                        ));
                    }
                }
                Ensure::FileDoesntExist { path } => {
                    let path = Path::new(path);
                    if path.exists() && path.is_file() {
                        errors.push(anyhow::anyhow!(
                            "File '{}' for task '{}' exists",
                            path.display(),
                            task.name()
                        ));
                    }
                }
            }

            debug!(
                "ensuring task visitor: checking task: {}: found {} errors",
                &task.name(),
                errors.len() - last_len
            );
            last_len = errors.len();
        }
        Ok(errors)
    }
}
