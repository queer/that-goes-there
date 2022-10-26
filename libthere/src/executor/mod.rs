use anyhow::Result;
use async_trait::async_trait;

use tokio::sync::Mutex;

use crate::plan;

pub mod simple;

pub type Logs = Vec<String>;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum PartialLogStream {
    Next(Logs),
    End,
}

#[async_trait]
pub trait Executor<'a, T: ExecutionContext + std::fmt::Debug = simple::SimpleExecutionContext<'a>>:
    std::fmt::Debug + Send + Sync
{
    async fn execute(&mut self, ctx: Mutex<&'a mut T>) -> Result<()>;

    fn tasks_completed(&self) -> Result<u32>;
}

#[async_trait]
pub trait ExecutionContext: std::fmt::Debug {
    fn name(&self) -> &str;

    fn plan(&self) -> &plan::Plan;
}

#[async_trait]
pub trait LogSink: std::fmt::Debug {
    async fn sink(&mut self, logs: PartialLogStream) -> Result<usize>;
}

#[async_trait]
pub trait LogSource: std::fmt::Debug {
    async fn source(&mut self) -> Result<PartialLogStream>;
}
