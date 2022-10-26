use std::collections::HashMap;

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
}

impl Host {
    pub fn new<S: Into<String>>(host: S) -> Self {
        Self {
            host: host.into(),
            port: default_ssh_port(),
            executor: "ssh".into(),
        }
    }

    pub fn new_with_port<S: Into<String>>(host: S, port: Port) -> Self {
        Self {
            host: host.into(),
            port,
            executor: "ssh".into(),
        }
    }

    pub fn new_with_executor<S: Into<String>>(host: S, executor: S) -> Self {
        Self {
            host: host.into(),
            port: default_ssh_port(),
            executor: executor.into(),
        }
    }

    pub fn new_with_port_and_executor<S: Into<String>>(host: S, port: Port, executor: S) -> Self {
        Self {
            host: host.into(),
            port,
            executor: executor.into(),
        }
    }
}
