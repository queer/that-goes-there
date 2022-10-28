use std::marker::PhantomData;
use std::sync::Arc;

use anyhow::Result;
use derive_getters::Getters;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::log::*;

pub mod host;
pub mod visitor;

pub use visitor::TaskVisitor;

use self::host::{Host, HostConfig};

/// An unplanned set of [`Task`]s to be executed. [`TaskSet`]s have no
/// validations applied to them outside of ensuring that they parse into
/// `Task`s. Validations necessary for applying a `Task` are generated during
/// the planning phase.
#[derive(Getters, Debug, Clone, Serialize, Deserialize)]
pub struct TaskSet {
    name: String,
    tasks: Vec<Task>,
}

impl TaskSet {
    pub fn new<S: Into<String>>(name: S) -> Self {
        Self {
            name: name.into(),
            tasks: vec![],
        }
    }

    #[tracing::instrument]
    pub fn add_task(&mut self, task: Task) {
        debug!("task set: added task to plan: {}", &task.name());
        self.tasks.push(task);
    }

    /// Generate a [`Plan`] from this `TaskSet`. Consumes this `TaskSet`.
    #[tracing::instrument]
    pub async fn plan(mut self) -> Result<Plan> {
        debug!("task set: planning tasks");
        let mut visitor = visitor::PlanningTaskVisitor::new(self.name.clone());
        for task in self.tasks.iter_mut() {
            task.accept(&mut visitor).await?;
        }
        debug!("task set: finished planning tasks");
        Ok(Plan::new(self.name, visitor.plan().clone()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Task {
    /// A command to be executed.
    Command {
        name: String,
        command: String,
        hosts: Vec<String>,
    },
    /// A directory to be created.
    CreateDirectory {
        name: String,
        path: String,
        hosts: Vec<String>,
    },
    /// A file to be created.
    TouchFile {
        name: String,
        path: String,
        hosts: Vec<String>,
    },
}

impl Task {
    #[tracing::instrument]
    pub async fn accept(&mut self, visitor: &mut dyn visitor::TaskVisitor<Out = ()>) -> Result<()> {
        visitor.visit_task(self)
    }

    pub fn name(&self) -> &str {
        match self {
            Task::Command { name, .. } => name,
            Task::CreateDirectory { name, .. } => name,
            Task::TouchFile { name, .. } => name,
            _ => "<unknown>",
        }
    }

    pub fn hosts(&self) -> Vec<String> {
        match self {
            Task::Command { hosts, .. } => hosts.clone(),
            Task::CreateDirectory { hosts, .. } => hosts.clone(),
            Task::TouchFile { hosts, .. } => hosts.clone(),
            _ => vec![],
        }
    }
}

/// A planned [`TaskSet`]. A `Plan` is a set of [`PlannedTask`]s that have had
/// their [`Ensure`]s generated for execution on the remote hosts.
#[derive(Getters, Debug, Clone, Deserialize, Serialize)]
pub struct Plan {
    name: String,
    blueprint: Vec<PlannedTask>,
}

impl Plan {
    pub fn new(name: String, blueprint: Vec<PlannedTask>) -> Self {
        Self { name, blueprint }
    }

    /// Generate a new plan for the given host. Does not consume this plan. Any
    /// [`Task`]s in this plan that do not apply to the specified host will be
    /// excluded from the resulting plan.
    #[tracing::instrument]
    pub fn plan_for_host(&self, host: &String, hosts: &HostConfig) -> Plan {
        Plan {
            name: self.name.clone(),
            blueprint: self
                .blueprint
                .iter()
                .filter_map(|task| {
                    let group_names: Vec<String> = hosts
                        .groups()
                        .iter()
                        .filter(|(_name, hosts)| hosts.contains(host))
                        .map(|(name, _hosts)| name.clone())
                        .collect();
                    if task.hosts().contains(host)
                        || task.hosts().iter().any(|host| group_names.contains(host))
                    {
                        Some(task.clone())
                    } else {
                        None
                    }
                })
                .collect(),
        }
    }
}

/// A planned [`Task`]. Contains information like the [`Ensure`]s necessary to
/// validate this command.
#[derive(Getters, Debug, Clone, Deserialize, Serialize)]
pub struct PlannedTask {
    name: String,
    command: Vec<String>,
    ensures: Vec<Ensure>,
    hosts: Vec<String>,
}

impl PlannedTask {
    #[tracing::instrument]
    pub fn from_shell_command<S: Into<String> + std::fmt::Debug>(
        name: S,
        command: S,
        hosts: Vec<String>,
    ) -> Result<Self> {
        let split = shell_words::split(command.into().as_str())?;
        let head = split[0].clone();
        Ok(Self {
            name: name.into(),
            ensures: vec![Ensure::ExeExists { exe: head }],
            command: split,
            hosts,
        })
    }
}

/// Validations for a [`Task`]. These are generated during the planning phase.
/// [`Ensure`]s are compiled down to shell commands executed on the remote host
/// prior to any `Task` commands, to ensure that the `Task` can be executed.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum Ensure {
    /// Ensure that the specified path exists on the host and is a file.
    FileExists { path: String },
    /// Ensure that the specified path exists on the host and is a directory.
    DirectoryExists { path: String },
    /// Ensure that the specified file does not exist on the host. Will fail if
    /// the path exists and is a directory.
    FileDoesntExist { path: String },
    /// Ensure that the specified directory does not exist on the host. Will
    /// fail if the path exists and is a file.
    DirectoryDoesntExist { path: String },
    /// Ensure that the specified executable exists on the host.
    ExeExists { exe: String },
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use crate::plan::host::HostConfig;

    use super::{Task, TaskSet};

    #[tokio::test]
    async fn test_that_tasks_can_be_planned() -> Result<()> {
        let mut taskset = TaskSet::new("test");
        taskset.add_task(Task::Command {
            name: "test".into(),
            command: "echo hello".into(),
            hosts: vec![],
        });
        let mut plan = taskset.plan().await?;
        assert_eq!(1, plan.blueprint().len());
        assert_eq!("test", plan.blueprint()[0].name());
        assert_eq!("echo hello", plan.blueprint()[0].command().join(" "));
        Ok(())
    }
}
