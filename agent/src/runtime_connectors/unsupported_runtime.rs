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

use std::{collections::HashMap, path::PathBuf};

use async_trait::async_trait;
use common::objects::{AgentName, WorkloadInstanceName, WorkloadSpec};

use crate::workload_state::WorkloadStateSender;

use super::{
    dummy_state_checker::DummyStateChecker, ReusableWorkloadState, RuntimeConnector, RuntimeError,
};

#[derive(Clone)]
pub struct UnsupportedRuntime(pub String);

#[async_trait]
impl RuntimeConnector<String, DummyStateChecker> for UnsupportedRuntime {
    fn name(&self) -> String {
        self.0.clone()
    }

    async fn get_reusable_workloads(
        &self,
        _agent_name: &AgentName,
    ) -> Result<Vec<ReusableWorkloadState>, RuntimeError> {
        Ok(Vec::new())
    }

    async fn create_workload(
        &self,
        runtime_workload_config: WorkloadSpec,
        _reusable_workload_id: Option<String>,
        _control_interface_path: Option<PathBuf>,
        _update_state_tx: WorkloadStateSender,
        _workload_file_path_mapping: HashMap<PathBuf, PathBuf>,
    ) -> Result<(String, DummyStateChecker), RuntimeError> {
        if runtime_workload_config.runtime == self.0 {
            Err(RuntimeError::Unsupported("Unsupported Runtime".into()))
        } else {
            Err(RuntimeError::Unsupported(format!(
                "Received a spec for the wrong runtime: '{}'",
                runtime_workload_config.runtime
            )))
        }
    }

    async fn get_workload_id(
        &self,
        _instance_name: &WorkloadInstanceName,
    ) -> Result<String, RuntimeError> {
        Err(RuntimeError::List(
            "Cannot get information about workload with unsupported runtime".into(),
        ))
    }

    async fn start_checker(
        &self,
        _workload_id: &String,
        _runtime_workload_config: WorkloadSpec,
        _update_state_tx: WorkloadStateSender,
    ) -> Result<DummyStateChecker, RuntimeError> {
        Ok(DummyStateChecker())
    }

    async fn delete_workload(&self, _workload_id: &String) -> Result<(), RuntimeError> {
        Ok(())
    }
}
