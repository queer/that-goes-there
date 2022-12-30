use serde::{Deserialize, Serialize};

use crate::plan::host::HostConfig;
use crate::plan::Plan;

#[doc(hidden)]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct JobStartRequest {
    pub plan: Plan,
    pub hosts: HostConfig,
}
