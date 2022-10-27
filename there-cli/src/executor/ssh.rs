use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use derive_getters::Getters;
use libthere::executor::simple::{SimpleLogSink, SimpleLogTx};
use libthere::executor::{ExecutionContext, Executor, LogSink, PartialLogStream};
use libthere::log::*;
use libthere::plan;
use libthere::plan::host::Host;
use tokio::sync::Mutex;

#[derive(Getters, Debug)]
pub struct SshExecutor<'a> {
    #[getter(skip)]
    log_sink: Arc<Mutex<SimpleLogSink<'a>>>,
    keypair: Arc<thrussh_keys::key::KeyPair>,
    host: &'a Host,
    hostname: String,
    tasks_completed: u32,
}

impl<'a> SshExecutor<'a> {
    #[tracing::instrument(skip(ssh_key, ssh_key_passphrase))]
    pub fn new(
        host: &'a Host,
        hostname: String,
        tx: &'a SimpleLogTx,
        ssh_key: String,
        ssh_key_passphrase: Option<String>,
    ) -> Result<Self> {
        let keypair = match ssh_key_passphrase {
            Some(passphrase) => {
                thrussh_keys::decode_secret_key(ssh_key.as_str(), Some(&passphrase))
                    .context("Decoding SSH key with passphrase failed.")
            }
            None => thrussh_keys::decode_secret_key(ssh_key.as_str(), None)
                .context("Decoding SSH key failed."),
        }?;

        Ok(Self {
            log_sink: Arc::new(Mutex::new(SimpleLogSink::new(tx))),
            keypair: Arc::new(keypair),
            host,
            hostname,
            tasks_completed: 0,
        })
    }

    #[tracing::instrument(skip(self))]
    async fn sink_one<S: Into<String> + std::fmt::Debug>(&mut self, msg: S) -> Result<usize> {
        self.log_sink
            .lock()
            .await
            .sink_one(msg.into().clone())
            .await
            .context("failed to sink log message.")
    }

    #[tracing::instrument(skip(self))]
    async fn sink_partial(&mut self, partial: PartialLogStream) -> Result<usize> {
        self.log_sink
            .lock()
            .await
            .sink(partial)
            .await
            .context("failed to sink log message.")
    }

    #[tracing::instrument(skip(self, session))]
    async fn ensure_task(
        &mut self,
        task: &'a plan::PlannedTask,
        session: &mut thrussh::client::Handle<SshClient>,
    ) -> Result<Vec<&'a plan::Ensure>> {
        let mut failures = vec![];
        for ensure in task.ensures() {
            self.sink_one(format!("ensuring {:?}", ensure)).await?;
            let pass = match ensure {
                plan::Ensure::DirectoryDoesntExist { path } => {
                    self.execute_command(format!("test -d \"{}\"", path), session)
                        .await?
                        != 0
                }
                plan::Ensure::DirectoryExists { path } => {
                    self.execute_command(format!("test -d \"{}\"", path), session)
                        .await?
                        == 0
                }
                plan::Ensure::FileDoesntExist { path } => {
                    self.execute_command(format!("test -f \"{}\"", path), session)
                        .await?
                        != 0
                }
                plan::Ensure::FileExists { path } => {
                    self.execute_command(format!("test -f \"{}\"", path), session)
                        .await?
                        == 0
                }
                plan::Ensure::ExeExists { exe } => {
                    self.execute_command(format!("which \"{}\"", exe), session)
                        .await?
                        == 0
                }
            };
            if !pass {
                failures.push(ensure);
            }
        }
        Ok(failures)
    }

    #[tracing::instrument(skip(self, session))]
    async fn execute_command<S: std::fmt::Debug + Clone + Into<String>>(
        &mut self,
        command: S,
        session: &mut thrussh::client::Handle<SshClient>,
    ) -> Result<u32> {
        println!("exec: {}", command.clone().into());
        // TODO: Figure out env etc...
        let mut channel = session
            .channel_open_session()
            .await
            .context("failed to open channel.")?;
        channel.exec(true, command.clone().into()).await?;

        while let Some(frame) = channel.wait().await {
            let mut sink = self.log_sink.lock().await;
            match frame {
                thrussh::ChannelMsg::Data { data } => {
                    sink.sink(PartialLogStream::Next(
                        String::from_utf8(data[..].to_vec())?
                            .split('\n')
                            .map(|s| s.to_string())
                            .collect(),
                    ))
                    .await?;
                }
                thrussh::ChannelMsg::ExtendedData { data, ext: _ } => {
                    sink.sink(PartialLogStream::Next(
                        String::from_utf8(data[..].to_vec())?
                            .split('\n')
                            .map(|s| s.to_string())
                            .collect(),
                    ))
                    .await?;
                }
                thrussh::ChannelMsg::Eof => {} // TODO: ???
                thrussh::ChannelMsg::Close => {
                    break;
                }
                thrussh::ChannelMsg::XonXoff { client_can_do: _ } => {} // TODO
                thrussh::ChannelMsg::ExitStatus { exit_status } => {
                    if exit_status == 0 {
                        return Ok(0);
                    } else {
                        return Ok(exit_status);
                    }
                }
                thrussh::ChannelMsg::ExitSignal {
                    // TODO
                    signal_name: _,
                    core_dumped: _,
                    error_message: _,
                    lang_tag: _,
                } => {}
                thrussh::ChannelMsg::WindowAdjusted { new_size: _ } => {}
                thrussh::ChannelMsg::Success => {}
            }
        }
        Ok(511)
    }

    #[tracing::instrument(skip(self, session))]
    async fn execute_task(
        &mut self,
        task: &'a plan::PlannedTask,
        ctx: &mut SshExecutionContext<'a>,
        session: &mut thrussh::client::Handle<SshClient>,
    ) -> Result<()> {
        self.sink_one(format!("** executing task: {}", task.name()))
            .await?;
        info!("executing task: {}", task.name());
        let failed_ensures = self.ensure_task(task, session).await?;
        if !failed_ensures.is_empty() {
            self.sink_one(format!("ensures failed: {:?}", failed_ensures))
                .await?;
            anyhow::bail!("ensures failed: {:?}", failed_ensures);
        }
        self.execute_command(task.command().join(" "), session)
            .await?;
        info!("task '{}' finished", task.name());
        self.sink_one(String::new()).await?;

        Ok(())
    }
}

#[async_trait]
impl<'a> Executor<'a, SshExecutionContext<'a>> for SshExecutor<'a> {
    #[tracing::instrument(skip(self, ctx))]
    async fn execute(&mut self, ctx: Mutex<&'a mut SshExecutionContext>) -> Result<()> {
        debug!("awaiting ctx lock...");
        let ctx = ctx.lock().await;
        debug!("got it!");
        self.sink_one(format!("* steps: {}", ctx.plan().blueprint().len()))
            .await?;

        // Attempt to get a working SSH client first; don't waste time.
        let sh = SshClient;
        let config = thrussh::client::Config {
            connection_timeout: Some(std::time::Duration::from_secs(5)),
            ..Default::default()
        };
        let config = Arc::new(config);
        let addr = format!("{}:{}", self.host.host(), self.host.port());
        debug!("connecting to {}", &addr);
        let mut session = thrussh::client::connect(config, addr, sh).await?;
        let auth_res = session
            .authenticate_publickey(self.host.real_remote_user(), self.keypair.clone())
            .await;
        if auth_res? {
            debug!("successfully authenticated!");

            // Actually apply the plan.
            let clone = ctx.clone();
            info!("applying plan: {}", ctx.plan().name());
            for task in ctx.plan.blueprint().iter() {
                debug!("ssh executor: executing task: {}", task.name());
                self.execute_task(task, &mut clone.clone(), &mut session)
                    .await
                    .with_context(|| format!("failed executing ssh task: {}", task.name()))?;
                self.tasks_completed += 1;
            }
            info!("plan applied: {}", ctx.plan().name());
            self.sink_one(format!(
                "*** finished applying plan: {} -> {} ({}/{})",
                ctx.plan().name(),
                &self.hostname,
                self.tasks_completed,
                ctx.plan().blueprint().len(),
            ))
            .await?;
            self.sink_partial(PartialLogStream::End).await?;
            Ok(())
        } else {
            self.sink_one("ssh authentication failed!".to_string())
                .await?;
            self.sink_partial(PartialLogStream::End).await?;
            anyhow::bail!("ssh authentication failed!");
        }
    }

    fn tasks_completed(&self) -> Result<u32> {
        Ok(self.tasks_completed)
    }
}

#[derive(Getters, Debug, Clone)]
pub struct SshExecutionContext<'a> {
    name: String,
    plan: &'a plan::Plan,
}

impl<'a> SshExecutionContext<'a> {
    pub fn new<S: Into<String>>(name: S, plan: &'a plan::Plan) -> Self {
        Self {
            name: name.into(),
            plan,
        }
    }
}

#[async_trait]
impl<'a> ExecutionContext for SshExecutionContext<'a> {
    fn name(&self) -> &str {
        &self.name
    }

    fn plan(&self) -> &plan::Plan {
        self.plan
    }
}

struct SshClient;

impl thrussh::client::Handler for SshClient {
    type Error = anyhow::Error;
    type FutureUnit =
        futures::future::Ready<Result<(Self, thrussh::client::Session), anyhow::Error>>;
    type FutureBool = futures::future::Ready<Result<(Self, bool), anyhow::Error>>;

    #[tracing::instrument(skip(self))]
    fn finished_bool(self, b: bool) -> Self::FutureBool {
        futures::future::ready(Ok((self, b)))
    }

    #[tracing::instrument(skip(self, session))]
    fn finished(self, session: thrussh::client::Session) -> Self::FutureUnit {
        futures::future::ready(Ok((self, session)))
    }

    #[tracing::instrument(skip(self))]
    fn check_server_key(
        self,
        _server_public_key: &thrussh_keys::key::PublicKey,
    ) -> Self::FutureBool {
        self.finished_bool(true)
    }
}
