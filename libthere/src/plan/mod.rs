use std::marker::PhantomData;
use std::sync::Arc;

use anyhow::Result;
use derive_getters::Getters;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::log::*;

pub mod visitor;
pub use visitor::{PlannedTaskVisitor, TaskVisitor};

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
    },
    CreateDirectory {
        name: String,
        path: String,
    },
    TouchFile {
        name: String,
        path: String,
    },
}

impl Task {
    #[tracing::instrument]
    pub async fn accept(
        &mut self,
        visitor: &mut dyn visitor::TaskVisitor<Out = ()>,
    ) -> Result<()> {
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
    pub async fn validate(&mut self) -> Result<(Plan, Vec<anyhow::Error>)> {
        use std::ops::DerefMut;
        use std::sync::Arc;
        use tokio::sync::Mutex;

        let me = self.clone();
        debug!("plan: validating plan: {}", &self.name());

        let mut errors = vec![];

        let mut visitor: Arc<Mutex<dyn visitor::PlannedTaskVisitor<Out = Vec<anyhow::Error>>>> =
            Arc::new(Mutex::new(visitor::EnsuringTaskVisitor::new()));
        for task in self.blueprint.iter_mut() {
            // TODO: ugh
            let fake_task = task.clone();
            let name = fake_task.name();

            debug!("plan: validating task: {}", &name);
            let visitor = visitor.clone();
            let mut visitor = visitor.lock().await;
            let task_errors = task.accept(visitor.deref_mut()).await?;
            let mut counter = 0;
            for err in task_errors {
                counter += 1;
                errors.push(err);
            }
            debug!("plan: validating task: {}: {} errors", &name, counter);
        }

        Ok((me, errors))
    }
}

#[derive(Getters, Debug, Clone, Deserialize, Serialize)]
pub struct PlannedTask {
    name: String,
    command: Vec<String>,
    ensures: Vec<Ensure>,
}

impl PlannedTask {
    #[tracing::instrument(skip(visitor))]
    pub async fn accept(
        &mut self,
        visitor: &mut dyn visitor::PlannedTaskVisitor<Out = Vec<anyhow::Error>>,
    ) -> Result<Vec<anyhow::Error>> {
        visitor.visit_planned_task(self).await
    }

    pub fn from_shell_command<S: Into<String>>(name: S, command: S) -> Result<Self> {
        let split = shell_words::split(command.into().as_str())?;
        let head = split[0].clone();
        Ok(Self {
            name: name.into(),
            ensures: vec![Ensure::ExeExists { exe: head }],
            command: split,
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

    use super::{Task, TaskSet};

    #[tokio::test]
    async fn test_that_tasks_can_be_planned() -> Result<()> {
        let mut taskset = TaskSet::new("test");
        taskset.add_task(Task::Command {
            name: "test".into(),
            command: "echo hello".into(),
        });
        let mut plan = taskset.plan().await?;
        assert_eq!(1, plan.blueprint().len());
        assert_eq!("test", plan.blueprint()[0].name());
        assert_eq!("echo hello", plan.blueprint()[0].command().join(" "));

        let (_, errors) = plan.validate().await?;
        assert!(errors.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn test_that_tasks_without_valid_executables_fail_planning() -> Result<()> {
        let mut taskset = TaskSet::new("test");
        taskset.add_task(Task::Command {
            name: "test".into(),
            command: "doesnotexist".into(),
        });
        let mut plan = taskset.plan().await?;
        let (_, errors) = plan.validate().await?;
        assert_eq!(1, errors.len());
        assert_eq!(
            "doesnotexist not found in $PATH".to_string(),
            format!("{}", errors[0])
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_that_touch_file_tasks_pass_validation() -> Result<()> {
        let mut taskset = TaskSet::new("test");
        taskset.add_task(Task::TouchFile {
            name: "test".into(),
            path: "./tmp/test.txt".into(),
        });
        let mut plan = taskset.plan().await?;
        let (_, errors) = plan.validate().await?;
        assert!(errors.is_empty());
        Ok(())
    }
}
