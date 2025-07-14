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
// [impl->swdd~agent-skips-unknown-runtime~2]
pub struct UnsupportedRuntime(pub String);

// [impl->swdd~agent-skips-unknown-runtime~2]
#[async_trait]
impl RuntimeConnector<String, DummyStateChecker<String>> for UnsupportedRuntime {
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
    ) -> Result<(String, DummyStateChecker<String>), RuntimeError> {
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
    ) -> Result<DummyStateChecker<String>, RuntimeError> {
        Ok(DummyStateChecker::new())
    }

    async fn delete_workload(&self, _workload_id: &String) -> Result<(), RuntimeError> {
        Ok(())
    }
}

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use crate::runtime_connectors::RuntimeConnector;

    use super::{RuntimeError, UnsupportedRuntime};
    use common::objects::{AgentName, WorkloadInstanceName, WorkloadSpec};
    use std::collections::HashMap;

    const TEST_RUNTIME_NAME: &str = "test_runtime";

    // [utest->swdd~agent-skips-unknown-runtime~2]
    #[tokio::test]
    async fn utest_name_returns_runtime_name() {
        let unsupported_runtime = UnsupportedRuntime(TEST_RUNTIME_NAME.to_string());

        assert_eq!(unsupported_runtime.name(), TEST_RUNTIME_NAME.to_string());
    }

    // [utest->swdd~agent-skips-unknown-runtime~2]
    #[tokio::test]
    async fn utest_get_reusable_workloads_returns_empty_vec() {
        let unsupported_runtime = UnsupportedRuntime(TEST_RUNTIME_NAME.to_string());
        let agent_name = AgentName::from("dummy_agent");

        let result = unsupported_runtime
            .get_reusable_workloads(&agent_name)
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }

    // [utest->swdd~agent-skips-unknown-runtime~2]
    #[tokio::test]
    async fn utest_create_workload_returns_unsupported_error_for_matching_runtime() {
        let unsupported_runtime = UnsupportedRuntime(TEST_RUNTIME_NAME.to_string());

        let workload_spec = WorkloadSpec {
            runtime: TEST_RUNTIME_NAME.to_string(),
            ..WorkloadSpec::default()
        };

        let result = unsupported_runtime
            .create_workload(
                workload_spec,
                None,
                None,
                tokio::sync::mpsc::channel(1).0,
                HashMap::new(),
            )
            .await;

        assert!(matches!(
            result,
            Err(RuntimeError::Unsupported(msg)) if msg == "Unsupported Runtime"
        ));
    }

    // [utest->swdd~agent-skips-unknown-runtime~2]
    #[tokio::test]
    async fn utest_create_workload_returns_unsupported_error_for_different_runtime() {
        let unsupported_runtime = UnsupportedRuntime(TEST_RUNTIME_NAME.to_string());
        let workload_spec = WorkloadSpec {
            runtime: "different_runtime".to_string(),
            ..WorkloadSpec::default()
        };

        let result = unsupported_runtime
            .create_workload(
                workload_spec,
                None,
                None,
                tokio::sync::mpsc::channel(1).0,
                HashMap::new(),
            )
            .await;

        assert!(matches!(
            result,
            Err(RuntimeError::Unsupported(msg)) if msg.contains("Received a spec for the wrong runtime")
        ));
    }

    // [utest->swdd~agent-skips-unknown-runtime~2]
    #[tokio::test]
    async fn utest_get_workload_id_returns_list_error() {
        let unsupported_runtime = UnsupportedRuntime(TEST_RUNTIME_NAME.to_string());
        let instance_name = WorkloadInstanceName::new("test-agent", "test-workload", "test-id");

        let result = unsupported_runtime.get_workload_id(&instance_name).await;

        assert!(matches!(
            result,
            Err(RuntimeError::List(msg)) if msg.contains("Cannot get information about workload")
        ));
    }

    // [utest->swdd~agent-skips-unknown-runtime~2]
    #[tokio::test]
    async fn utest_start_checker_returns_dummy_checker() {
        let unsupported_runtime = UnsupportedRuntime(TEST_RUNTIME_NAME.to_string());
        let workload_id = "test_id".to_string();
        let workload_spec = WorkloadSpec::default();

        let result = unsupported_runtime
            .start_checker(&workload_id, workload_spec, tokio::sync::mpsc::channel(1).0)
            .await;

        assert!(result.is_ok());
    }

    // [utest->swdd~agent-skips-unknown-runtime~2]
    #[tokio::test]
    async fn utest_delete_workload_returns_ok() {
        let unsupported_runtime = UnsupportedRuntime(TEST_RUNTIME_NAME.to_string());
        let workload_id = "test_id".to_string();

        let result = unsupported_runtime.delete_workload(&workload_id).await;

        assert!(result.is_ok());
    }
}
