use std::marker::PhantomData;
use std::sync::Arc;

use anyhow::Result;
use derive_getters::Getters;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::log::*;

pub mod host;
pub mod visitor;

pub use visitor::{PlannedTaskVisitor, TaskVisitor};

use self::host::{Host, HostConfig};

#[derive(Getters, Debug, Clone, Serialize, Deserialize)]
pub struct TaskSet {
    name: String,
    tasks: Vec<Task>,
}

impl TaskSet {
    pub fn new<S: Into<String>>(name: S) -> Self {
        Self {
            name: name.into(),
            tasks: Vec::new(),
        }
    }

    #[tracing::instrument]
    pub fn add_task(&mut self, task: Task) {
        debug!("task set: added task to plan: {}", &task.name());
        self.tasks.push(task);
    }

    #[tracing::instrument]
    pub async fn plan(&mut self) -> Result<Plan> {
        debug!("task set: planning tasks");
        let mut visitor = visitor::PlanningTaskVisitor::new(self.name.clone());
        for task in self.tasks.iter_mut() {
            task.accept(&mut visitor).await?;
        }
        debug!("task set: finished planning tasks");
        Ok(Plan::new(self.name.clone(), visitor.plan().clone()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Task {
    Command {
        name: String,
        command: String,
        hosts: Vec<String>,
    },
    CreateDirectory {
        name: String,
        path: String,
        hosts: Vec<String>,
    },
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

#[derive(Getters, Debug, Clone, Deserialize, Serialize)]
pub struct Plan {
    name: String,
    blueprint: Vec<PlannedTask>,
}

impl Plan {
    pub fn new(name: String, blueprint: Vec<PlannedTask>) -> Self {
        Self { name, blueprint }
    }

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

#[derive(Getters, Debug, Clone, Deserialize, Serialize)]
pub struct PlannedTask {
    name: String,
    command: Vec<String>,
    ensures: Vec<Ensure>,
    hosts: Vec<String>,
}

impl PlannedTask {
    #[tracing::instrument(skip(visitor))]
    pub async fn accept(
        &mut self,
        visitor: &mut dyn visitor::PlannedTaskVisitor<Out = Vec<anyhow::Error>>,
    ) -> Result<Vec<anyhow::Error>> {
        visitor.visit_planned_task(self).await
    }

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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum Ensure {
    FileExists { path: String },
    DirectoryExists { path: String },
    FileDoesntExist { path: String },
    DirectoryDoesntExist { path: String },
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
