use anyhow::Result;
use async_trait::async_trait;

use tokio::sync::Mutex;

use crate::plan::host::Host;
use crate::plan::Plan;

pub mod simple;

pub type Logs = Vec<String>;

/// A partial log stream. Used for controlling how logs are streamed through
/// the [`Executor`] -> [`LogSink`] -> [`LogSource`] pipeline.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum PartialLogStream {
    /// The next logs to emit.
    Next(Logs),
    /// The end of the stream. Must cause the [`LogSink`] and [`LogSource`] to
    /// close.
    End,
}

/// An `Executor` is responsible for executing a given [`Plan`] on its target
/// hosts. It is possible, but not necessarily required, that the `Plan` passed
/// into the [`ExecutionContext`] contains more than one [`Host`].
#[async_trait]
pub trait Executor<'a, T: ExecutionContext + std::fmt::Debug = simple::SimpleExecutionContext<'a>>:
    std::fmt::Debug + Send + Sync
{
    async fn execute(&mut self, ctx: Mutex<&'a mut T>) -> Result<()>;

    fn tasks_completed(&self) -> Result<u32>;
}

/// The context for a given [`Executor`]'s execution of its plan. Contains the
/// plan being executed by the `Executor`.
#[async_trait]
pub trait ExecutionContext: std::fmt::Debug {
    /// The name of this execution. Usually the name of the [`Plan`].
    fn name(&self) -> &str;

    /// The plan being executed.
    fn plan(&self) -> &Plan;
}

/// A sink for logs from an [`Executor`]. The `Executor` will push logs into
/// its `LogSink`, and the code calling the `Executor` is responsible for
/// pulling those logs out of the [`LogSource`] on the other end.
#[async_trait]
pub trait LogSink: std::fmt::Debug {
    /// Sink a [`PartialLogStream`] into this sink. Returns the number of logs
    /// successfully sunk.
    async fn sink(&mut self, logs: PartialLogStream) -> Result<usize>;

    /// Sink a single [`String`] into the log. Returns the number of logs
    /// successfully sunk (probably just `1`).
    #[tracing::instrument(skip(self))]
    async fn sink_one<S: Into<String> + Send + std::fmt::Debug>(
        &mut self,
        log: S,
    ) -> Result<usize> {
        self.sink(PartialLogStream::Next(vec![log.into()])).await
    }
}

/// A source for logs from an [`Executor`]. This end of the logging pipeline is
/// responsible for pulling logs out of the [`LogSink`] on the other end.
#[async_trait]
pub trait LogSource: std::fmt::Debug {
    /// Read the next [`PartialLogStream`] from the logs streaming into this
    /// source from the [`LogSink`] on the other end.
    async fn source(&mut self) -> Result<PartialLogStream>;
}
