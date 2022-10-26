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
pub trait Executor<T: ExecutionContext + std::fmt::Debug = simple::SimpleExecutionContext>:
    std::fmt::Debug + Send + Sync
{
    async fn execute(&self, ctx: Mutex<&mut T>) -> Result<()>;
}

#[async_trait]
pub trait ExecutionContext: std::fmt::Debug {
    fn name(&self) -> &str;

    fn plan(&self) -> &plan::Plan;
}

#[async_trait]
pub trait LogSink: std::fmt::Debug {
    async fn sink(&mut self, logs: PartialLogStream) -> Result<()>;
}

#[async_trait]
pub trait LogSource: std::fmt::Debug {
    async fn source(&mut self) -> Result<PartialLogStream>;
}
