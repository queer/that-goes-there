//! A visitor for a [`Task`] in a [`TaskSet`]. Task visitors are used to do
//! things like compile the [`Plan`] for a `TaskSet`.

use std::path::Path;

use async_trait::async_trait;
use color_eyre::eyre::Result;
use derive_getters::Getters;

use super::{Ensure, Plan, PlannedTask, Task, TaskSet};
use crate::log::*;

/// A visitor for a [`Task`] in a [`TaskSet`]. Task visitors are used to do
/// things like compile the [`Plan`] for a `TaskSet`.
pub trait TaskVisitor: Send + std::fmt::Debug {
    /// The output type of this visitor.
    type Out;

    /// Visit the given task. This is used for things like compiling a `Plan`.
    fn visit_task(&mut self, task: &Task) -> Result<Self::Out>;
}

/// An implementation of [`TaskVisitor`] that compiles a [`Task`] into a
/// `Vec<PlannedTask>`.
#[derive(Getters, Debug, Clone)]
pub struct PlanningTaskVisitor<'a> {
    name: &'a str,
    plan: Vec<PlannedTask>,
}

impl<'a> PlanningTaskVisitor<'a> {
    /// Create a new visitor.
    pub fn new(name: &'a str) -> Self {
        Self { name, plan: vec![] }
    }
}

impl<'a> TaskVisitor for PlanningTaskVisitor<'a> {
    type Out = ();

    /// Visits the given task and compiles it into a [`PlannedTask`] that is
    /// stored in the visitor's state.
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
                    ensures: vec![
                        Ensure::ExeExists {
                            exe: "touch".into(),
                        },
                        Ensure::DirectoryExists {
                            path: Path::new(path).parent().unwrap().display().to_string(),
                        },
                    ],
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
