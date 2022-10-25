use std::path::Path;

use anyhow::{Context, Result};
use derive_getters::Getters;

use super::{Ensure, PlannedTask, Task};

pub trait TaskVisitor<'a>: std::fmt::Debug {
    type Out;

    fn visit_task(&mut self, task: &'a Task) -> Result<Self::Out>;
}

#[derive(Getters, Debug, Clone)]
pub struct PlanningTaskVisitor<'a> {
    name: String,
    plan: Vec<PlannedTask<'a>>,
}

impl<'a> PlanningTaskVisitor<'a> {
    pub fn new(name: String) -> Self {
        Self {
            name,
            plan: Vec::new(),
        }
    }
}

impl<'a> TaskVisitor<'a> for PlanningTaskVisitor<'a> {
    type Out = ();

    #[tracing::instrument]
    fn visit_task(&mut self, task: &'a Task) -> Result<Self::Out> {
        match task {
            Task::Command { name, command } => {
                self.plan.push(PlannedTask {
                    name,
                    command: command.clone(),
                    ensures: vec![Ensure::ExeExists {
                        exe: command[0],
                    }],
                });
            }
            Task::CreateDirectory { name, path } => {
                self.plan.push(PlannedTask {
                    name,
                    command: vec!["mkdir", path],
                    ensures: vec![Ensure::DirectoryExists { path }],
                });
            }
            Task::TouchFile { name, path } => {
                self.plan.push(PlannedTask {
                    name,
                    command: vec!["touch", path],
                    ensures: vec![Ensure::ExeExists { exe: "touch" }],
                });
            }
            Task::__phantom(_) => unreachable!(),
        }
        Ok(())
    }
}

pub trait PlannedTaskVisitor: std::fmt::Debug {
    type Out;

    fn visit_planned_task(&mut self, task: &PlannedTask) -> Result<Self::Out>;
}

#[derive(Getters, Debug, Clone)]
pub struct EnsuringTaskVisitor {}

impl EnsuringTaskVisitor {
    pub fn new() -> Self {
        Self {}
    }
}

impl PlannedTaskVisitor for EnsuringTaskVisitor {
    type Out = Vec<anyhow::Error>;

    #[tracing::instrument]
    fn visit_planned_task(&mut self, task: &PlannedTask) -> Result<Self::Out> {
        let mut errors: Vec<anyhow::Error> = vec![];
        for ensure in task.ensures() {
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
        }
        Ok(errors)
    }
}
