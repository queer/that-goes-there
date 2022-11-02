use std::collections::HashMap;

use color_eyre::eyre::Result;
use derive_getters::Getters;
use serde::{Deserialize, Serialize};

pub type Port = u16;

pub fn default_ssh_port() -> Port {
    22
}

#[derive(Getters, Debug, Clone, Serialize, Deserialize, Default)]
pub struct HostConfig {
    hosts: HashMap<String, Host>,
    groups: HashMap<String, Vec<String>>,
}

#[derive(Getters, Debug, Clone, Serialize, Deserialize)]
pub struct Host {
    host: String,
    #[serde(default = "self::default_ssh_port")]
    port: Port,
    executor: String,
    remote_user: Option<String>,
}

impl Host {
    pub fn real_remote_user(&self) -> String {
        #[allow(clippy::or_fun_call)]
        self.remote_user.clone().unwrap_or("root".to_string())
    }
}
