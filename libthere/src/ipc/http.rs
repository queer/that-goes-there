use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::plan::host::HostConfig;
use crate::plan::Plan;

#[doc(hidden)]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct JobStartRequest {
    pub plan: Plan,
    pub hosts: HostConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobState {
    pub logs: HashMap<String, Vec<LogEntry>>,
    pub plan: Plan,
    pub hosts: HostConfig,
    pub status: JobStatus,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub enum JobStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogEntry {
    pub hostname: String,
    pub log: String,
}
