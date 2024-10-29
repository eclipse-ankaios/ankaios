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

use std::str::FromStr;

use async_trait::async_trait;

use common::objects::{ExecutionState, WorkloadSpec};

#[cfg(test)]
use mockall::automock;

use crate::workload_state::WorkloadStateSender;

// [impl->swdd~agent-general-runtime-state-getter-interface~1]
#[async_trait]
#[cfg_attr(test, automock)]
pub trait RuntimeStateGetter<WorkloadId>: Send + Sync + 'static
where
    WorkloadId: ToString + FromStr + Clone + Send + Sync + 'static,
{
    // [impl->swdd~allowed-workload-states~2]
    async fn get_state(&self, workload_id: &WorkloadId) -> ExecutionState;
}

// [impl->swdd~agent-general-state-checker-interface~1]
#[async_trait]
pub trait StateChecker<WorkloadId>
where
    WorkloadId: ToString + FromStr + Clone + Send + Sync + 'static,
{
    fn start_checker(
        workload_spec: &WorkloadSpec,
        workload_id: WorkloadId,
        manager_interface: WorkloadStateSender,
        state_getter: impl RuntimeStateGetter<WorkloadId>,
    ) -> Self;
    async fn stop_checker(self);
}
