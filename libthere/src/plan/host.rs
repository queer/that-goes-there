//! Host configuration for a [`Plan`].

use std::collections::HashMap;

use color_eyre::eyre::Result;
use derive_getters::Getters;
use serde::{Deserialize, Serialize};

/// A port number.
pub type Port = u16;

/// The default SSH port.
pub fn default_ssh_port() -> Port {
    22
}

/// A set of hosts and groups that can be used to execute a [`Plan`].
#[derive(Getters, Debug, Clone, Serialize, Deserialize, Default)]
pub struct HostConfig {
    hosts: HashMap<String, Host>,
    groups: HashMap<String, Vec<String>>,
}

/// A single host in a [`HostConfig`].
#[derive(Getters, Debug, Clone, Serialize, Deserialize)]
pub struct Host {
    host: String,
    #[serde(default = "self::default_ssh_port")]
    port: Port,
    executor: String,
    remote_user: Option<String>,
}

impl Host {
    /// Get the real remote user for this host. If the remote user is not set,
    /// return `root`.
    pub fn real_remote_user(&self) -> String {
        #[allow(clippy::or_fun_call)]
        self.remote_user.clone().unwrap_or("root".to_string())
    }
}
