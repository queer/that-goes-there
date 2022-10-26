use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use derive_getters::Getters;
use libthere::executor::simple::{SimpleLogSink, SimpleLogTx};
use libthere::executor::{ExecutionContext, Executor, LogSink, PartialLogStream};
use libthere::log::*;
use libthere::plan;
use tokio::sync::Mutex;

#[derive(Getters, Debug)]
pub struct SshExecutor<'a> {
    #[getter(skip)]
    log_sink: Arc<Mutex<SimpleLogSink<'a>>>,
    ssh_user: String,
    keypair: Arc<thrussh_keys::key::KeyPair>,
}

impl<'a> SshExecutor<'a> {
    pub fn new(
        tx: &'a SimpleLogTx,
        ssh_user: String,
        ssh_key: String,
        ssh_key_passphrase: Option<String>,
    ) -> Self {
        let keypair = match ssh_key_passphrase {
            Some(passphrase) => {
                thrussh_keys::decode_secret_key(ssh_key.as_str(), Some(&passphrase))
                    .context("Decoding SSH key with passphrase failed.")
            }
            None => thrussh_keys::decode_secret_key(ssh_key.as_str(), None)
                .context("Decoding SSH key failed."),
        }
        .unwrap();

        Self {
            log_sink: Arc::new(Mutex::new(SimpleLogSink::new(tx))),
            ssh_user,
            keypair: Arc::new(keypair),
        }
    }

    #[tracing::instrument(skip(self, channel))]
    async fn execute_task(
        &self,
        task: &plan::PlannedTask,
        ctx: &mut SshExecutionContext,
        channel: &mut thrussh::client::Channel,
    ) -> Result<()> {
        println!("** Executing task: {}", task.name());
        info!("executing task: {}", task.name());

        // TODO: Figure out env etc...
        channel.exec(true, task.command().join(" ")).await?;

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
                thrussh::ChannelMsg::Close => break,
                thrussh::ChannelMsg::XonXoff { client_can_do: _ } => {} // TODO
                thrussh::ChannelMsg::ExitStatus { exit_status } => {
                    if exit_status == 0 {
                        break;
                    } else {
                        anyhow::bail!(
                            "command '{}' exited with status {}",
                            task.command()[0],
                            exit_status
                        );
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

        info!("task '{}' finished", task.name());
        let mut sink = self.log_sink.lock().await;
        sink.sink(PartialLogStream::Next(vec![String::new()]))
            .await?;
        sink.sink(PartialLogStream::End).await?;

        Ok(())
    }
}

#[async_trait]
impl<'a> Executor<SshExecutionContext> for SshExecutor<'a> {
    async fn execute(&self, ctx: Mutex<&mut SshExecutionContext>) -> Result<()> {
        // Attempt to get a working SSH client first; don't waste time.
        let sh = SshClient;
        let config = Arc::new(thrussh::client::Config::default());
        let addr = "localhost:2222";
        debug!("connecting to {}", &addr);
        let mut session = thrussh::client::connect(config, addr, sh).await.unwrap();
        let auth_res = session
            .authenticate_publickey(self.ssh_user.as_str(), self.keypair.clone())
            .await;
        if auth_res? {
            debug!("successfully authenticated!");
            debug!("awaiting channel session open...");
            let mut channel = session.channel_open_session().await?;
            debug!("channel open!");

            // Actually apply the plan.
            debug!("awaiting ctx lock...");
            let ctx = ctx.lock().await;
            debug!("got it!");
            let clone = ctx.clone();
            println!("* Applying plan: {}", ctx.plan().name());
            println!("* Steps: {}", ctx.plan().blueprint().len());
            info!("applying plan: {}", ctx.plan().name());
            for task in ctx.plan.blueprint().iter() {
                debug!("ssh executor: executing task: {}", task.name());
                self.execute_task(task, &mut clone.clone(), &mut channel)
                    .await
                    .with_context(|| format!("failed executing ssh task: {}", task.name()))?;
            }
            info!("plan applied: {}", ctx.plan().name());
            println!("* Finished applying plan: {}", ctx.plan().name());
        } else {
            let mut sink = self.log_sink.lock().await;
            sink.sink(PartialLogStream::Next(vec![
                "ssh authentication failed!".to_string()
            ]))
            .await?;
            sink.sink(PartialLogStream::End).await?;
        }

        Ok(())
    }
}

#[derive(Getters, Debug, Clone)]
pub struct SshExecutionContext {
    name: String,
    plan: plan::Plan,
}

impl SshExecutionContext {
    pub fn new<S: Into<String>>(name: S, plan: plan::Plan) -> Self {
        Self {
            name: name.into(),
            plan,
        }
    }
}

#[async_trait]
impl ExecutionContext for SshExecutionContext {
    fn name(&self) -> &str {
        &self.name
    }

    fn plan(&self) -> &plan::Plan {
        &self.plan
    }
}

struct SshClient;

impl thrussh::client::Handler for SshClient {
    type Error = anyhow::Error;
    type FutureUnit =
        futures::future::Ready<Result<(Self, thrussh::client::Session), anyhow::Error>>;
    type FutureBool = futures::future::Ready<Result<(Self, bool), anyhow::Error>>;

    fn finished_bool(self, b: bool) -> Self::FutureBool {
        futures::future::ready(Ok((self, b)))
    }

    fn finished(self, session: thrussh::client::Session) -> Self::FutureUnit {
        futures::future::ready(Ok((self, session)))
    }

    fn check_server_key(
        self,
        _server_public_key: &thrussh_keys::key::PublicKey,
    ) -> Self::FutureBool {
        self.finished_bool(true)
    }
}
