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
            Task::Command { name, command, .. } => {
                let mut final_command = vec![];
                for shell_word in shell_words::split(command)? {
                    final_command.push(shell_word.clone());
                }
                self.plan.push(PlannedTask::from_shell_command(
                    name,
                    command,
                    task.hosts(),
                )?);
            }
            Task::CreateDirectory { name, path, .. } => {
                self.plan.push(PlannedTask {
                    name: name.to_string(),
                    command: vec!["mkdir".into(), path.to_string()],
                    ensures: vec![Ensure::DirectoryExists {
                        path: path.to_string(),
                    }],
                    hosts: task.hosts(),
                });
            }
            Task::TouchFile { name, path, .. } => {
                self.plan.push(PlannedTask {
                    name: name.to_string(),
                    command: vec!["touch".into(), path.to_string()],
                    ensures: vec![Ensure::ExeExists {
                        exe: "touch".into(),
                    }],
                    hosts: task.hosts(),
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
