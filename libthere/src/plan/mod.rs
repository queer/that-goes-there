use anyhow::Result;
use derive_getters::Getters;

mod visitor;

#[derive(Getters, Debug, Clone)]
pub struct TaskSet {
    name: String,
    tasks: Vec<Task>,
}

#[derive(Debug, Clone)]
pub enum Task {
    Command { name: String, command: Vec<String> },
    CreateDirectory { name: String, path: String },
    TouchFile { name: String, path: String },
}

impl Task {
    #[tracing::instrument]
    pub async fn accept(&mut self, visitor: &mut dyn visitor::TaskVisitor<Out = ()>) -> Result<()> {
        visitor.visit_task(self)
    }
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
        self.tasks.push(task);
    }

    #[tracing::instrument]
    pub async fn plan(&mut self) -> Result<Plan> {
        let mut visitor = visitor::PlanningTaskVisitor::new(self.name.clone());
        for task in self.tasks.iter_mut() {
            task.accept(&mut visitor).await?;
        }
        Ok(Plan::new(self.name.clone(), visitor.plan().clone()))
    }
}

#[derive(Getters, Debug, Clone)]
pub struct Plan {
    name: String,
    blueprint: Vec<PlannedTask>,
}

impl Plan {
    pub fn new(name: String, blueprint: Vec<PlannedTask>) -> Self {
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
pub struct PlannedTask {
    name: String,
    command: Vec<String>,
    ensures: Vec<Ensure>,
}

impl PlannedTask {
    #[tracing::instrument]
    pub async fn accept(
        &mut self,
        visitor: &mut dyn visitor::PlannedTaskVisitor<Out = Vec<anyhow::Error>>,
    ) -> Result<Vec<anyhow::Error>> {
        visitor.visit_planned_task(self)
    }
}

#[derive(Debug, Clone)]
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
            name: "test".to_string(),
            command: vec!["echo".to_string(), "hello".to_string()],
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
            name: "test".to_string(),
            command: vec!["doesnotexist".to_string(), "hello".to_string()],
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
            name: "test".to_string(),
            path: "./tmp/test.txt".to_string(),
        });
        let mut plan = taskset.plan().await?;
        let errors = plan.validate().await?;
        assert!(errors.is_empty());
        Ok(())
    }
}
