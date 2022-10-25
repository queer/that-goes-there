use std::marker::PhantomData;
use std::sync::Arc;

use anyhow::Result;
use derive_getters::Getters;
use tokio::sync::Mutex;

pub mod visitor;
pub use visitor::{PlannedTaskVisitor, TaskVisitor};

#[derive(Getters, Debug, Clone)]
pub struct TaskSet<'a> {
    name: String,
    tasks: Vec<Task<'a>>,
    _phantom: PhantomData<&'a ()>,
}

impl<'a> TaskSet<'a> {
    pub fn new<S: Into<String>>(name: S) -> Self {
        Self {
            name: name.into(),
            tasks: Vec::new(),
            _phantom: PhantomData,
        }
    }

    #[tracing::instrument]
    pub fn add_task(&mut self, task: Task<'a>) {
        self.tasks.push(task);
    }

    #[tracing::instrument]
    pub async fn plan(&'a mut self) -> Result<Plan> {
        let mut visitor = visitor::PlanningTaskVisitor::new(self.name.clone());
        for task in self.tasks.iter_mut() {
            task.accept(&mut visitor).await?;
        }
        Ok(Plan::new(self.name.clone(), visitor.plan().clone()))
    }
}

#[derive(Debug, Clone)]
pub enum Task<'a> {
    Command {
        name: &'a str,
        command: Vec<&'a str>,
    },
    CreateDirectory {
        name: &'a str,
        path: &'a str,
    },
    TouchFile {
        name: &'a str,
        path: &'a str,
    },
    #[doc(hidden)]
    #[allow(non_camel_case_types)]
    __phantom(PhantomData<&'a ()>),
}

impl<'a> Task<'a> {
    #[tracing::instrument]
    pub async fn accept(
        &'a mut self,
        visitor: &mut dyn visitor::TaskVisitor<'a, Out = ()>,
    ) -> Result<()> {
        visitor.visit_task(self)
    }
}

#[derive(Getters, Debug, Clone)]
pub struct Plan<'a> {
    name: String,
    blueprint: Vec<PlannedTask<'a>>,
}

impl<'a> Plan<'a> {
    pub fn new(name: String, blueprint: Vec<PlannedTask<'a>>) -> Self {
        Self { name, blueprint }
    }

    #[tracing::instrument]
    pub async fn validate(&'a mut self) -> Result<(Plan<'a>, Vec<anyhow::Error>)> {
        use std::ops::DerefMut;
        use std::sync::Arc;
        use tokio::sync::Mutex;

        let me = self.clone();

        let mut errors = vec![];

        let mut visitor: Arc<Mutex<dyn visitor::PlannedTaskVisitor<'a, Out = Vec<anyhow::Error>>>> =
            Arc::new(Mutex::new(visitor::EnsuringTaskVisitor::new()));
        for task in self.blueprint.iter_mut() {
            let visitor = visitor.clone();
            let mut visitor = visitor.lock().await;
            let task_errors = task.accept(visitor.deref_mut()).await?;
            for err in task_errors {
                errors.push(err);
            }
        }

        Ok((me, errors))
    }
}

#[derive(Getters, Debug, Clone)]
pub struct PlannedTask<'a> {
    name: &'a str,
    command: Vec<&'a str>,
    ensures: Vec<Ensure<'a>>,
}

impl<'a> PlannedTask<'a> {
    #[tracing::instrument(skip(visitor))]
    pub async fn accept(
        &'a mut self,
        visitor: &mut dyn visitor::PlannedTaskVisitor<'a, Out = Vec<anyhow::Error>>,
    ) -> Result<Vec<anyhow::Error>> {
        visitor.visit_planned_task(self).await
    }
}

#[derive(Debug, Clone)]
pub enum Ensure<'a> {
    FileExists { path: &'a str },
    DirectoryExists { path: &'a str },
    FileDoesntExist { path: &'a str },
    DirectoryDoesntExist { path: &'a str },
    ExeExists { exe: &'a str },
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use super::{Task, TaskSet};

    #[tokio::test]
    async fn test_that_tasks_can_be_planned() -> Result<()> {
        let mut taskset = TaskSet::new("test");
        taskset.add_task(Task::Command {
            name: "test",
            command: vec!["echo", "hello"],
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
            name: "test",
            command: vec!["doesnotexist", "hello"],
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
            name: "test",
            path: "./tmp/test.txt",
        });
        let mut plan = taskset.plan().await?;
        let (_, errors) = plan.validate().await?;
        assert!(errors.is_empty());
        Ok(())
    }
}
