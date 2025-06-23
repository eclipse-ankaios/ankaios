// Copyright (c) 2025 Elektrobit Automotive GmbH
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
use common::objects::WorkloadSpec;

use crate::workload_state::WorkloadStateSender;

use super::{RuntimeStateGetter, StateChecker};

pub struct DummyStateChecker();

#[async_trait]
impl<WorkloadId> StateChecker<WorkloadId> for DummyStateChecker
where
    WorkloadId: ToString + FromStr + Clone + Send + Sync + 'static,
{
    fn start_checker(
        _workload_spec: &WorkloadSpec,
        _workload_id: WorkloadId,
        _manager_interface: WorkloadStateSender,
        _state_getter: impl RuntimeStateGetter<WorkloadId>,
    ) -> Self {
        Self()
    }
    async fn stop_checker(self) {}
}
