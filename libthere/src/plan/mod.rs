use std::marker::PhantomData;

use anyhow::Result;
use derive_getters::Getters;

mod visitor;

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
    Command { name: &'a str, command: Vec<&'a str> },
    CreateDirectory { name: &'a str, path: &'a str },
    TouchFile { name: &'a str, path: &'a str },
    #[doc(hidden)]
    #[allow(non_camel_case_types)]
    __phantom(PhantomData<&'a ()>),
}

impl<'a> Task<'a> {
    #[tracing::instrument]
    pub async fn accept(&'a mut self, visitor: &mut dyn visitor::TaskVisitor<'a, Out = ()>) -> Result<()> {
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
    pub async fn validate(&mut self) -> Result<Vec<anyhow::Error>> {
        let mut visitor = visitor::EnsuringTaskVisitor::new();
        let mut errors: Vec<anyhow::Error> = vec![];
        for task in self.blueprint.iter_mut() {
            let task_errors = task.accept(&mut visitor).await?;
            for err in task_errors {
                errors.push(err);
            }
        }

        Ok(errors)
    }
}

#[derive(Getters, Debug, Clone)]
pub struct PlannedTask<'a> {
    name: &'a str,
    command: Vec<&'a str>,
    ensures: Vec<Ensure<'a>>,
}

impl<'a> PlannedTask<'a> {
    #[tracing::instrument]
    pub async fn accept(
        &mut self,
        visitor: &mut dyn visitor::PlannedTaskVisitor<Out = Vec<anyhow::Error>>,
    ) -> Result<Vec<anyhow::Error>> {
        visitor.visit_planned_task(self)
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

        let errors = plan.validate().await?;
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
        let errors = plan.validate().await?;
        assert_eq!(1, errors.len());
        assert_eq!("doesnotexist not found in $PATH".to_string(), format!("{}", errors[0]));
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
        let errors = plan.validate().await?;
        assert!(errors.is_empty());
        Ok(())
    }
}
