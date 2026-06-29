// Copyright (c) 2023 Elektrobit Automotive GmbH
//
// This program and the accompanying materials are made available under the
// terms of the Apache License, Version 2.0 which is available at
// https://www.apache.org/licenses/LICENSE-2.0.
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
// WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
// License for the specific language governing permissions and limitations
// under the License.
//
// SPDX-License-Identifier: Apache-2.0

use ankaios_api::ank_base::ExecutionStateSpec;

use async_trait::async_trait;
#[cfg(test)]
use mockall::automock;
use std::str::FromStr;

use crate::runtime_connectors::RuntimeWorkloadId;

pub type StateCheckerHandle = Box<dyn StateChecker + Send + Sync>;

// [impl->swdd~agent-general-runtime-state-getter-interface~1]
#[async_trait]
#[cfg_attr(test, automock)]
pub trait RuntimeStateGetter: Send + Sync + 'static {
    // [impl->swdd~allowed-workload-states~2]
    async fn get_state(&self, workload_id: &RuntimeWorkloadId) -> ExecutionStateSpec;
}

// [impl->swdd~agent-general-state-checker-interface~1]
#[async_trait]
pub trait StateChecker: Send + Sync {
    async fn stop_checker(self: Box<Self>);
}
