use anyhow::Result;
use async_trait::async_trait;

use tokio::sync::Mutex;

use crate::plan;

pub mod simple;

pub type Logs = Vec<String>;

#[async_trait]
pub trait Executor<T: ExecutionContext + std::fmt::Debug>: std::fmt::Debug {
    async fn execute(&self, ctx: Mutex<&mut T>) -> Result<()>;
}

#[async_trait]
pub trait ExecutionContext: std::fmt::Debug {
    fn name(&self) -> &str;

    fn plan(&self) -> &plan::Plan;
}

#[async_trait]
pub trait LogSink: std::fmt::Debug {
    async fn sink(&mut self, logs: Logs) -> Result<()>;
}

#[async_trait]
pub trait LogSource: std::fmt::Debug {
    async fn source(&mut self) -> Result<Logs>;
}
