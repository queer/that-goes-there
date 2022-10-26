use std::cell::RefCell;
use std::sync::Arc;
use std::time::Duration;
use std::{future::Future, marker::PhantomData};

use anyhow::{Context, Result};
use async_trait::async_trait;
use derive_getters::Getters;
use tokio::sync::{mpsc, Mutex};

use super::{ExecutionContext, Executor, LogSink, LogSource, Logs, PartialLogStream};
use crate::log::*;
use crate::plan;

pub type SimpleLogTx = mpsc::Sender<PartialLogStream>;
pub type SimpleLogRx = mpsc::Receiver<PartialLogStream>;

#[derive(Getters, Debug, Clone)]
pub struct SimpleExecutor<'a> {
    #[getter(skip)]
    log_sink: Arc<Mutex<SimpleLogSink<'a>>>,
    tasks_completed: u32,
}

impl<'a> SimpleExecutor<'a> {
    pub fn new(tx: &'a SimpleLogTx) -> Self {
        Self {
            log_sink: Arc::new(Mutex::new(SimpleLogSink::new(tx))),
            tasks_completed: 0,
        }
    }

    #[tracing::instrument(skip(self))]
    async fn execute_task(
        &mut self,
        task: &plan::PlannedTask,
        ctx: &mut SimpleExecutionContext<'a>,
    ) -> Result<()> {
        use std::ops::Deref;
        use std::process::Stdio;

        use tokio::io::{AsyncRead, AsyncReadExt};
        use tokio::process::Command;
        use tokio_stream::StreamExt;
        use tokio_util::codec::{BytesCodec, FramedRead};

        println!("** Executing task: {}", task.name());
        info!("executing task: {}", task.name());
        let cmd = &task.command()[0];
        let args = task.command()[1..].to_vec();

        let mut builder = Command::new(cmd);
        let mut builder = builder
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .args(args);
        // TODO: env etc

        let mut child = builder
            .spawn()
            .with_context(|| format!("spawning command '{}' failed", cmd))?;

        let mut stdout = FramedRead::new(child.stdout.take().unwrap(), BytesCodec::new());
        let mut stderr = FramedRead::new(child.stderr.take().unwrap(), BytesCodec::new());
        let sink_clone = self.log_sink.clone();

        while let Ok(None) = child.try_wait() {
            let mut sink = sink_clone.lock().await;
            tokio::select! {
                Some(next) = stdout.next() => {
                    if let Ok(logs) = next {
                        let logs = vec![String::from_utf8(logs.to_vec()).unwrap_or_else(|d| format!("got: {:#?}", d))];
                        match sink.sink(PartialLogStream::Next(logs)).await {
                            Ok(_) => {}
                            Err(err) => {
                                error!("error sinking logs: {}", err);
                            }
                        }
                    }
                }
                Some(next) = stderr.next() => {
                    if let Ok(logs) = next {
                        let logs = vec![String::from_utf8(logs.to_vec()).unwrap_or_else(|d| format!("got: {:#?}", d))];
                        match sink.sink(PartialLogStream::Next(logs)).await {
                            Ok(_) => {}
                            Err(err) => {
                                error!("error sinking logs: {}", err);
                            }
                        }
                    }
                }
                else => {
                    break;
                }
            }
        }

        info!("task '{}' finished", task.name());
        let mut sink = self.log_sink.lock().await;
        sink.sink(PartialLogStream::Next(vec![String::new()]))
            .await?;
        sink.sink(PartialLogStream::End).await?;
        self.tasks_completed += 1;

        Ok(())
    }
}

#[async_trait]
impl<'a> Executor<'a, SimpleExecutionContext<'a>> for SimpleExecutor<'a> {
    #[tracing::instrument(skip(self))]
    async fn execute(&mut self, ctx: Mutex<&'a mut SimpleExecutionContext>) -> Result<()> {
        let mut ctx = ctx.lock().await;
        let clone = ctx.clone();
        println!("* applying plan: {}", ctx.plan().name());
        println!("* steps: {}", ctx.plan().blueprint().len());
        info!("applying plan: {}", ctx.plan().name());
        for task in ctx.plan.blueprint().iter() {
            debug!("simple executor: executing task: {}", task.name());
            self.execute_task(task, &mut clone.clone()).await?;
        }
        info!("plan applied: {}", ctx.plan().name());
        println!(
            "* finished applying plan: {} ({}/{})",
            ctx.plan().name(),
            self.tasks_completed(),
            ctx.plan().blueprint().len()
        );
        Ok(())
    }

    fn tasks_completed(&self) -> Result<u32> {
        Ok(self.tasks_completed)
    }
}

#[derive(Getters, Debug, Clone)]
pub struct SimpleExecutionContext<'a> {
    name: String,
    plan: &'a plan::Plan,
}

impl<'a> SimpleExecutionContext<'a> {
    pub fn new<S: Into<String>>(name: S, plan: &'a plan::Plan) -> Self {
        Self {
            name: name.into(),
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
}

#[derive(Getters, Debug, Clone)]
pub struct SimpleLogSink<'a> {
    tx: &'a SimpleLogTx,
}

impl<'a> SimpleLogSink<'a> {
    pub fn new(tx: &'a SimpleLogTx) -> Self {
        Self { tx }
    }
}

#[async_trait]
impl<'a> LogSink for SimpleLogSink<'a> {
    #[tracing::instrument]
    async fn sink(&mut self, logs: PartialLogStream) -> Result<usize> {
        let out = match logs {
            PartialLogStream::Next(ref logs) => Ok(logs.len()),
            PartialLogStream::End => Ok(0),
        };
        self.tx.send(logs).await.context("Failed sending logs")?;
        out
    }
}

#[derive(Debug)]
pub struct SimpleLogSource {
    rx: SimpleLogRx,
    ended: bool,
}

impl SimpleLogSource {
    pub fn new(rx: SimpleLogRx) -> Self {
        Self { rx, ended: false }
    }
}

#[async_trait]
impl LogSource for SimpleLogSource {
    #[tracing::instrument]
    async fn source(&mut self) -> Result<PartialLogStream> {
        if self.ended {
            anyhow::bail!("Log source already ended");
        }
        let mut out = vec![];
        match &self.rx.try_recv() {
            Ok(partial_stream) => match partial_stream {
                PartialLogStream::Next(logs) => {
                    for log in logs {
                        out.push(log.clone());
                    }
                }
                PartialLogStream::End => {
                    self.ended = true;
                }
            },
            Err(mpsc::error::TryRecvError::Empty) => {}
            Err(mpsc::error::TryRecvError::Disconnected) => {
                return Err(anyhow::anyhow!("sink lost"));
            }
        }
        if self.ended {
            Ok(PartialLogStream::End)
        } else {
            Ok(PartialLogStream::Next(out))
        }
    }
}

#[cfg(test)]
mod test {
    use crate::executor::simple::*;
    use crate::executor::*;
    use crate::plan::host::HostConfig;
    use crate::plan::*;

    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_simple_executor() -> Result<()> {
        let (mut tx, mut rx) = mpsc::channel(69);
        let mut log_source = SimpleLogSource::new(rx);

        let mut taskset = TaskSet::new("test");
        taskset.add_task(Task::Command {
            name: "test".into(),
            command: "echo 'hello'".into(),
            hosts: vec![],
        });
        let mut plan = taskset.plan().await?;
        let hosts = HostConfig::default();
        let (plan, errors) = plan.validate(&hosts).await?;
        assert!(errors.is_empty());
        let mut ctx = SimpleExecutionContext::new("test", &plan);
        let mut executor = SimpleExecutor::new(&tx);
        executor.execute(Mutex::new(&mut ctx)).await?;
        assert_eq!(
            PartialLogStream::Next(vec!["hello\n".into()]),
            log_source.source().await?
        );
        assert_eq!(
            PartialLogStream::Next(vec![String::new()]),
            log_source.source().await?
        );
        assert_eq!(PartialLogStream::End, log_source.source().await?);
        Ok(())
    }
}
