use std::cell::RefCell;
use std::sync::Arc;
use std::{future::Future, marker::PhantomData};

use anyhow::{Context, Result};
use async_trait::async_trait;
use derive_getters::Getters;
use tokio::sync::{Mutex, mpsc};

use super::{ExecutionContext, Executor, Logs, LogSink, LogSource};
use crate::log::*;
use crate::plan;

pub type SimpleLogTx<'a> = mpsc::Sender<Logs>;
pub type SimpleLogRx<'a> = mpsc::Receiver<Logs>;

#[derive(Getters, Debug, Clone, Default)]
pub struct SimpleExecutor<'a> {
    _phantom: PhantomData<&'a ()>,
}

impl<'a> SimpleExecutor<'a> {
    pub fn new() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }

    #[tracing::instrument(skip(self))]
    async fn execute_task(&self, task: &'a plan::PlannedTask<'a>, ctx: &'a mut SimpleExecutionContext<'a>) -> Result<()> {
        use std::ops::Deref;
        use std::process::Stdio;

        use tokio::io::{AsyncRead, AsyncReadExt};
        use tokio::process::Command;
        use tokio_stream::StreamExt;
        use tokio_util::codec::{BytesCodec, FramedRead};

        info!("*** executing task: {}", task.name());
        let cmd = task.command()[0];
        let args = task.command()[1..].to_vec();

        let mut child = Command::new(cmd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .args(args)
            // TODO: env etc.
            .spawn()
            .with_context(|| format!("spawning command '{}' failed", cmd))?;

        let mut stdout = FramedRead::new(child.stdout.take().unwrap(), BytesCodec::new());
        let mut stderr = FramedRead::new(child.stderr.take().unwrap(), BytesCodec::new());

        while let Ok(None) = child.try_wait() {
            tokio::select! {
                _ = child.wait() => {
                    info!("*** task '{}' finished", task.name());
                    break;
                }
                stdout = stdout.next() => {
                    if let Some(Ok(logs)) = stdout {
                        let logs = vec![String::from_utf8(logs.to_vec())?];
                        ctx.sink_logs(logs).await.unwrap();
                    }
                }
                stderr = stderr.next() => {
                    if let Some(Ok(logs)) = stderr {
                        let logs = vec![String::from_utf8(logs.to_vec())?];
                        ctx.sink_logs(logs).await.unwrap();
                    }
                }
            }
        }

        Ok(())
    }
}

#[async_trait]
impl<'a> Executor<SimpleExecutionContext<'a>> for SimpleExecutor<'a> {
    #[tracing::instrument]
    async fn execute(&self, ctx: Mutex<&mut SimpleExecutionContext<'a>>) -> Result<()> {
        // TODO: What to do about all this cloning? ;-;
        let mut ctx = ctx.lock().await;
        let clone = ctx.clone();
        for task in ctx.plan.blueprint().iter() {
            debug!("simple executor: executing task: {}", task.name());
            self.execute_task(task, &mut clone.clone()).await?;
        }
        Ok(())
    }
}

#[derive(Getters, Debug, Clone)]
pub struct SimpleExecutionContext<'a> {
    name: String,
    plan: &'a plan::Plan<'a>,
    #[getter(skip)]
    log_sink: Arc<Mutex<SimpleLogSink>>,
}

impl<'a> SimpleExecutionContext<'a> {
    pub fn new<S: Into<String>>(name: S, plan: &'a plan::Plan<'a>, tx: SimpleLogTx<'a>) -> Self {
        Self {
            name: name.into(),
            log_sink: Arc::new(Mutex::new(SimpleLogSink::new(tx))),
            plan,
        }
    }
}

#[async_trait]
impl<'a> ExecutionContext for SimpleExecutionContext<'a> {
    fn name(&self) -> &str {
        &self.name
    }

    fn plan(&self) -> &plan::Plan {
        self.plan
    }

    #[tracing::instrument]
    async fn sink_logs(&mut self, logs: Logs) -> Result<()> {
        debug!("simple execution context: sinking {} logs", logs.len());
        self.log_sink.lock().await.sink(logs).await
    }
}

#[derive(Getters, Debug, Clone)]
struct SimpleLogSink {
    tx: SimpleLogTx<'static>,
}

impl SimpleLogSink {
    pub fn new(tx: SimpleLogTx) -> Self {
        Self { tx }
    }
}

#[async_trait]
impl LogSink for SimpleLogSink {
    #[tracing::instrument]
    async fn sink(&mut self, logs: Logs) -> Result<()> {
        debug!("simple log sink: sinking {} logs", logs.len());
        self.tx.send(logs).await.context("Failed sending logs")?;
        Ok(())
    }
}

#[derive(Debug)]
struct SimpleLogSource {
    rx: SimpleLogRx<'static>,
}

impl SimpleLogSource {
    pub fn new(rx: SimpleLogRx<'static>) -> Self {
        Self { rx }
    }
}

#[async_trait]
impl LogSource for SimpleLogSource {
    #[tracing::instrument]
    async fn source(&mut self) -> Result<Logs> {
        debug!("simple log source: sourcing logs");
        let mut out = vec![];
        loop {
            match self.rx.try_recv() {
                Ok(logs) => {
                    out.extend(logs);
                }
                Err(mpsc::error::TryRecvError::Empty) => {
                    break;
                }
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    break;
                }
            }
        }
        debug!("simple log source: sourced {} logs", &out.len());
        Ok(out)
    }
}

#[cfg(test)]
mod test {
    use crate::executor::simple::*;
    use crate::executor::*;
    use crate::plan::*;

    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_simple_executor() -> Result<()> {
        let (tx, rx) = mpsc::channel(69);
        let mut log_source = SimpleLogSource::new(rx);

        let mut taskset = TaskSet::new("test");
        taskset.add_task(Task::Command {
            name: "test",
            command: vec!["echo", "hello"],
        });
        let mut plan = taskset.plan().await?;
        let (plan, errors) = plan.validate().await?;
        assert!(errors.is_empty());
        let mut ctx = SimpleExecutionContext::new("test", &plan, tx);
        let mut executor = SimpleExecutor::new();
        executor.execute(Mutex::new(&mut ctx)).await?;
        let logs = log_source.source().await?;
        assert_eq!(vec!["hello\n"], logs);
        Ok(())
    }
}
