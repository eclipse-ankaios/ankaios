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

use async_trait::async_trait;
use common::{
    objects::{AgentName, ExecutionState, WorkloadInstanceName, WorkloadSpec, WorkloadState},
    std_extensions::IllegalStateResult,
};
#[cfg(test)]
use mockall::automock;

#[cfg_attr(test, mockall_double::double)]
use crate::control_interface::ControlInterface;

#[cfg_attr(test, mockall_double::double)]
use crate::control_interface::control_interface_info::ControlInterfaceInfo;

use crate::{
    runtime_connectors::{OwnableRuntime, RuntimeError, StateChecker},
    workload_state::{WorkloadStateSender, WorkloadStateSenderInterface},
};

use crate::workload::control_loop_state::ControlLoopState;
#[cfg_attr(test, mockall_double::double)]
use crate::workload::workload_control_loop::WorkloadControlLoop;
#[cfg_attr(test, mockall_double::double)]
use crate::workload::Workload;
use crate::workload::WorkloadCommandSender;

use tokio::task::JoinHandle;

#[async_trait]
#[cfg_attr(test, automock)]
pub trait RuntimeFacade: Send + Sync + 'static {
    async fn get_reusable_workloads(
        &self,
        agent_name: &AgentName,
    ) -> Result<Vec<WorkloadState>, RuntimeError>;

    fn create_workload(
        &self,
        runtime_workload: WorkloadSpec,
        control_interface_info: Option<ControlInterfaceInfo>,
        update_state_tx: &WorkloadStateSender,
    ) -> Workload;

    fn resume_workload(
        &self,
        runtime_workload: WorkloadSpec,
        control_interface: Option<ControlInterfaceInfo>,
        update_state_tx: &WorkloadStateSender,
    ) -> Workload;

    fn delete_workload(
        &self,
        instance_name: WorkloadInstanceName,
        update_state_tx: &WorkloadStateSender,
        report_workload_states_for_workload: bool,
    );
}

pub struct GenericRuntimeFacade<
    WorkloadId: ToString + Send + Sync + 'static,
    StChecker: StateChecker<WorkloadId> + Send + Sync,
> {
    runtime: Box<dyn OwnableRuntime<WorkloadId, StChecker>>,
}

impl<WorkloadId, StChecker> GenericRuntimeFacade<WorkloadId, StChecker>
where
    WorkloadId: ToString + Send + Sync + 'static,
    StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
{
    pub fn new(runtime: Box<dyn OwnableRuntime<WorkloadId, StChecker>>) -> Self {
        GenericRuntimeFacade { runtime }
    }
}

#[async_trait]
impl<
        WorkloadId: ToString + Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
    > RuntimeFacade for GenericRuntimeFacade<WorkloadId, StChecker>
{
    // [impl->swdd~agent-facade-forwards-list-reusable-workloads-call~1]
    async fn get_reusable_workloads(
        &self,
        agent_name: &AgentName,
    ) -> Result<Vec<WorkloadState>, RuntimeError> {
        log::debug!(
            "Searching for reusable '{}' workloads on agent '{}'.",
            self.runtime.name(),
            agent_name
        );
        self.runtime.get_reusable_workloads(agent_name).await
    }

    // [impl->swdd~agent-create-workload~2]
    fn create_workload(
        &self,
        workload_spec: WorkloadSpec,
        control_interface_info: Option<ControlInterfaceInfo>,
        update_state_tx: &WorkloadStateSender,
    ) -> Workload {
        let (_task_handle, workload) = Self::create_workload_non_blocking(
            self,
            workload_spec,
            control_interface_info,
            update_state_tx,
        );
        workload
    }

    // [impl->swdd~agent-resume-workload~2]
    fn resume_workload(
        &self,
        workload_spec: WorkloadSpec,
        control_interface_info: Option<ControlInterfaceInfo>,
        update_state_tx: &WorkloadStateSender,
    ) -> Workload {
        let (_task_handle, workload) = Self::resume_workload_non_blocking(
            self,
            workload_spec,
            control_interface_info,
            update_state_tx,
        );
        workload
    }

    // [impl->swdd~agent-delete-old-workload~2]
    // [impl->swdd~agent-delete-old-workload-without-sending-workload-states~1]
    fn delete_workload(
        &self,
        instance_name: WorkloadInstanceName,
        update_state_tx: &WorkloadStateSender,
        report_workload_states_for_workload: bool,
    ) {
        let _task_handle = Self::delete_workload_non_blocking(
            self,
            instance_name,
            update_state_tx,
            report_workload_states_for_workload,
        );
    }
}

impl<
        WorkloadId: ToString + Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
    > GenericRuntimeFacade<WorkloadId, StChecker>
{
    // [impl->swdd~agent-create-workload~2]
    fn create_workload_non_blocking(
        &self,
        workload_spec: WorkloadSpec,
        control_interface_info: Option<ControlInterfaceInfo>,
        update_state_tx: &WorkloadStateSender,
    ) -> (JoinHandle<()>, Workload) {
        let runtime = self.runtime.to_owned();
        let update_state_tx = update_state_tx.clone();
        let workload_name = workload_spec.instance_name.workload_name().to_owned();

        let (control_interface_path, control_interface) = if let Some(info) = control_interface_info
        {
            let run_folder = info.get_run_folder().clone();
            let output_pipe_sender = info.get_to_server_sender();
            let instance_name = info.get_instance_name().clone();
            let authorizer = info.move_authorizer();
            match ControlInterface::new(&run_folder, &instance_name, output_pipe_sender, authorizer)
            {
                Ok(result) => {
                    log::info!(
                        "Successfully created control interface for workload '{}'.",
                        workload_name
                    );
                    (
                        Some(workload_spec.instance_name.pipes_folder_name(&run_folder)),
                        Some(result),
                    )
                }
                Err(err) => {
                    log::warn!(
                        "Could not create control interface when creating workload '{}': '{}'",
                        workload_name,
                        err
                    );
                    (None, None)
                }
            }
        } else {
            log::debug!(
                "Skipping creation of control interface for workload '{}'.",
                workload_name
            );
            (None, None)
        };

        log::debug!(
            "Creating '{}' workload '{}'.",
            runtime.name(),
            workload_name,
        );
        let (workload_command_tx, workload_command_receiver) = WorkloadCommandSender::new();
        let workload_command_sender = workload_command_tx.clone();
        let task_handle = tokio::spawn(async move {
            workload_command_sender
                .create()
                .await
                .unwrap_or_else(|err| {
                    log::warn!("Failed to send workload command retry: '{}'", err);
                });

            let control_loop_state = ControlLoopState::builder()
                .workload_spec(workload_spec)
                .control_interface_path(control_interface_path)
                .workload_state_sender(update_state_tx)
                .runtime(runtime)
                .workload_command_receiver(workload_command_receiver)
                .retry_sender(workload_command_sender)
                .build()
                .unwrap_or_illegal_state();

            WorkloadControlLoop::run(control_loop_state).await
        });

        (
            task_handle,
            Workload::new(workload_name, workload_command_tx, control_interface),
        )
    }

    // [impl->swdd~agent-resume-workload~2]
    fn resume_workload_non_blocking(
        &self,
        workload_spec: WorkloadSpec,
        control_interface_info: Option<ControlInterfaceInfo>,
        update_state_tx: &WorkloadStateSender,
    ) -> (JoinHandle<()>, Workload) {
        let workload_name = workload_spec.instance_name.workload_name().to_owned();
        let runtime = self.runtime.to_owned();
        let update_state_tx = update_state_tx.clone();

        log::debug!(
            "Resuming '{}' workload '{}'.",
            runtime.name(),
            workload_name,
        );

        // [impl->swdd~agent-control-interface-created-for-eligible-workloads~1]
        let control_interface = control_interface_info.and_then(|info| { if workload_spec.needs_control_interface() {
            let run_folder = info.get_run_folder().clone();
            let output_pipe_sender = info.get_to_server_sender();
            let instance_name = info.get_instance_name().clone();
            let authorizer = info.move_authorizer();
            match ControlInterface::new(&run_folder, &instance_name, output_pipe_sender, authorizer)
            {
                Ok(result) => Some(result),
                Err(err) => {
                    log::warn!(
                                "Could not reuse or create control interface when resuming workload '{}': '{}'",
                                workload_spec.instance_name,
                                err
                            );
                    None
                }
            }
        } else {
            log::info!(
                    "No control interface access rights specified for workload '{}'. Skipping creation of control interface.",
                    workload_spec.instance_name.clone().workload_name()
                );
            None
        }});

        let (workload_command_tx, workload_command_receiver) = WorkloadCommandSender::new();
        let workload_command_sender = workload_command_tx.clone();
        let task_handle = tokio::spawn(async move {
            // let instance_name = workload_spec.instance_name.clone();
            workload_command_sender
                .resume()
                .await
                .unwrap_or_else(|err| {
                    log::warn!("Failed to send workload command retry: '{}'", err);
                });

            let control_loop_state = ControlLoopState::builder()
                .workload_spec(workload_spec)
                .workload_state_sender(update_state_tx)
                .runtime(runtime)
                .workload_command_receiver(workload_command_receiver)
                .retry_sender(workload_command_sender)
                .build()
                .unwrap_or_illegal_state();

            WorkloadControlLoop::run(control_loop_state).await
        });

        (
            task_handle,
            Workload::new(workload_name, workload_command_tx, control_interface),
        )
    }

    // [impl->swdd~agent-delete-old-workload~2]
    // [impl->swdd~agent-delete-old-workload-without-sending-workload-states~1]
    fn delete_workload_non_blocking(
        &self,
        instance_name: WorkloadInstanceName,
        update_state_tx: &WorkloadStateSender,
        /* The boolean flag to disable sending of workload states is a temporary workaround
        until direct start of bundles is implemented to prevent workload states
        from being overwritten by the delete. */
        report_workload_states_for_workload: bool,
    ) -> JoinHandle<()> {
        let runtime = self.runtime.to_owned();
        let update_state_tx = update_state_tx.clone();

        log::debug!(
            "Deleting '{}' workload '{}' on agent '{}'",
            runtime.name(),
            instance_name.workload_name(),
            instance_name.agent_name(),
        );

        tokio::spawn(async move {
            if report_workload_states_for_workload {
                update_state_tx
                    .report_workload_execution_state(
                        &instance_name,
                        ExecutionState::stopping_requested(),
                    )
                    .await;
            }

            if let Ok(id) = runtime.get_workload_id(&instance_name).await {
                if let Err(err) = runtime.delete_workload(&id).await {
                    if report_workload_states_for_workload {
                        update_state_tx
                            .report_workload_execution_state(
                                &instance_name,
                                ExecutionState::delete_failed(err),
                            )
                            .await;
                    }
                    return; // The early exit is needed to skip sending the removed message.
                }
            } else {
                log::debug!("Workload '{}' already gone.", instance_name);
            }

            if report_workload_states_for_workload {
                update_state_tx
                    .report_workload_execution_state(&instance_name, ExecutionState::removed())
                    .await;
            }
        })
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
    use common::objects::{
        generate_test_workload_spec_with_control_interface_access,
        generate_test_workload_spec_with_param, ExecutionState, WorkloadInstanceName,
        WorkloadState,
    };

    use crate::{
        control_interface::{
            authorizer::MockAuthorizer, control_interface_info::MockControlInterfaceInfo,
            MockControlInterface,
        },
        runtime_connectors::{
            runtime_connector::test::{MockRuntimeConnector, RuntimeCall, StubStateChecker},
            GenericRuntimeFacade, OwnableRuntime, RuntimeFacade,
        },
        workload::{ControlLoopState, MockWorkload, MockWorkloadControlLoop},
        workload_state::assert_execution_state_sequence,
    };

    const RUNTIME_NAME: &str = "runtime1";
    const AGENT_NAME: &str = "agent_x";
    const WORKLOAD_1_NAME: &str = "workload1";
    const WORKLOAD_ID: &str = "workload_id_1";
    const PIPES_LOCATION: &str = "/some/path";
    const TEST_CHANNEL_BUFFER_SIZE: usize = 20;

    // [utest->swdd~agent-facade-forwards-list-reusable-workloads-call~1]
    #[tokio::test]
    async fn utest_runtime_facade_reusable_running_workloads() {
        let mut runtime_mock = MockRuntimeConnector::new();

        let workload_instance_name = WorkloadInstanceName::builder()
            .workload_name(WORKLOAD_1_NAME)
            .build();

        let workload_state = WorkloadState {
            instance_name: workload_instance_name.clone(),
            execution_state: ExecutionState::initial(),
        };

        runtime_mock
            .expect(vec![RuntimeCall::GetReusableWorkloads(
                AGENT_NAME.into(),
                Ok(vec![workload_state]),
            )])
            .await;

        let ownable_runtime_mock: Box<dyn OwnableRuntime<String, StubStateChecker>> =
            Box::new(runtime_mock.clone());
        let test_runtime_facade = Box::new(GenericRuntimeFacade::<String, StubStateChecker>::new(
            ownable_runtime_mock,
        ));

        assert_eq!(
            test_runtime_facade
                .get_reusable_workloads(&AGENT_NAME.into())
                .await
                .unwrap()
                .iter()
                .map(|x| x.instance_name.clone())
                .collect::<Vec<WorkloadInstanceName>>(),
            vec![workload_instance_name]
        );

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-create-workload~2]
    #[tokio::test]
    async fn utest_runtime_facade_create_workload() {
        let _ = env_logger::builder().is_test(true).try_init();

        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let workload_spec = generate_test_workload_spec_with_control_interface_access(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let control_interface_mock = MockControlInterface::default();
        let control_interface_new_context = MockControlInterface::new_context();
        control_interface_new_context
            .expect()
            .once()
            .return_once(|_, _, _, _| Ok(control_interface_mock));

        let mut control_interface_info_mock = MockControlInterfaceInfo::default();
        control_interface_info_mock
            .expect_get_run_folder()
            .once()
            .return_const(PIPES_LOCATION.into());

        control_interface_info_mock
            .expect_get_to_server_sender()
            .once()
            .return_const(tokio::sync::mpsc::channel::<common::to_server_interface::ToServer>(1).0);

        control_interface_info_mock
            .expect_get_instance_name()
            .once()
            .return_const(workload_spec.instance_name.clone());

        control_interface_info_mock
            .expect_move_authorizer()
            .once()
            .return_once(MockAuthorizer::default);

        let (wl_state_sender, _wl_state_receiver) =
            tokio::sync::mpsc::channel(TEST_CHANNEL_BUFFER_SIZE);

        let mock_workload = MockWorkload::default();
        let new_workload_context = MockWorkload::new_context();
        new_workload_context
            .expect()
            .once()
            .return_once(|_, _, _| mock_workload);

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock.expect(vec![]).await;

        let ownable_runtime_mock: Box<dyn OwnableRuntime<String, StubStateChecker>> =
            Box::new(runtime_mock.clone());
        let test_runtime_facade = Box::new(GenericRuntimeFacade::<String, StubStateChecker>::new(
            ownable_runtime_mock,
        ));

        let mock_control_loop = MockWorkloadControlLoop::run_context();
        mock_control_loop
            .expect()
            .once()
            .return_once(|_: ControlLoopState<String, StubStateChecker>| ());

        let (task_handle, _workload) = test_runtime_facade.create_workload_non_blocking(
            workload_spec.clone(),
            Some(control_interface_info_mock),
            &wl_state_sender,
        );

        tokio::task::yield_now().await;

        assert!(task_handle.await.is_ok());
        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-resume-workload~2]
    // [utest->swdd~agent-control-interface-created-for-eligible-workloads~1]
    #[tokio::test]
    async fn utest_runtime_facade_resume_workload_with_control_interface_access() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let mut control_interface_info_mock = MockControlInterfaceInfo::default();
        control_interface_info_mock
            .expect_get_run_folder()
            .once()
            .return_const(PIPES_LOCATION.into());
        control_interface_info_mock
            .expect_get_to_server_sender()
            .once()
            .return_const(tokio::sync::mpsc::channel::<common::to_server_interface::ToServer>(1).0);
        control_interface_info_mock
            .expect_get_instance_name()
            .once()
            .return_const(
                WorkloadInstanceName::builder()
                    .workload_name(WORKLOAD_1_NAME)
                    .build(),
            );
        control_interface_info_mock
            .expect_move_authorizer()
            .once()
            .return_once(MockAuthorizer::default);

        let control_interface_new_context = MockControlInterface::new_context();
        control_interface_new_context
            .expect()
            .once()
            .return_once(|_, _, _, _| Ok(MockControlInterface::default()));

        let workload_spec = generate_test_workload_spec_with_control_interface_access(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let (wl_state_sender, _wl_state_receiver) =
            tokio::sync::mpsc::channel(TEST_CHANNEL_BUFFER_SIZE);

        let mock_control_loop = MockWorkloadControlLoop::run_context();
        mock_control_loop
            .expect()
            .once()
            .return_once(|_: ControlLoopState<String, StubStateChecker>| ());

        let mock_workload = MockWorkload::default();
        let new_workload_context = MockWorkload::new_context();
        new_workload_context
            .expect()
            .once()
            .return_once(|_, _, _| mock_workload);

        let runtime_mock = MockRuntimeConnector::new();

        let ownable_runtime_mock: Box<dyn OwnableRuntime<String, StubStateChecker>> =
            Box::new(runtime_mock.clone());
        let test_runtime_facade = Box::new(GenericRuntimeFacade::<String, StubStateChecker>::new(
            ownable_runtime_mock,
        ));

        let (task_handle, _workload) = test_runtime_facade.resume_workload_non_blocking(
            workload_spec.clone(),
            Some(control_interface_info_mock),
            &wl_state_sender,
        );

        tokio::task::yield_now().await;
        assert!(task_handle.await.is_ok());
        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-control-interface-created-for-eligible-workloads~1]
    #[tokio::test]
    async fn utest_runtime_facade_resume_workload_without_control_interface_access() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let control_interface_info_mock = MockControlInterfaceInfo::default();

        let control_interface_new_context = MockControlInterface::new_context();
        control_interface_new_context.expect().never();

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let (wl_state_sender, _wl_state_receiver) =
            tokio::sync::mpsc::channel(TEST_CHANNEL_BUFFER_SIZE);

        let mock_control_loop = MockWorkloadControlLoop::run_context();
        mock_control_loop
            .expect()
            .once()
            .return_once(|_: ControlLoopState<String, StubStateChecker>| ());

        let mock_workload = MockWorkload::default();
        let new_workload_context = MockWorkload::new_context();
        new_workload_context
            .expect()
            .once()
            .return_once(|_, _, _| mock_workload);

        let runtime_mock = MockRuntimeConnector::new();

        let ownable_runtime_mock: Box<dyn OwnableRuntime<String, StubStateChecker>> =
            Box::new(runtime_mock.clone());
        let test_runtime_facade = Box::new(GenericRuntimeFacade::<String, StubStateChecker>::new(
            ownable_runtime_mock,
        ));

        let (task_handle, _workload) = test_runtime_facade.resume_workload_non_blocking(
            workload_spec.clone(),
            Some(control_interface_info_mock),
            &wl_state_sender,
        );

        tokio::task::yield_now().await;
        assert!(task_handle.await.is_ok());
        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-delete-old-workload~2]
    #[tokio::test]
    async fn utest_runtime_facade_delete_workload() {
        let mut runtime_mock = MockRuntimeConnector::new();

        let (wl_state_sender, wl_state_receiver) =
            tokio::sync::mpsc::channel(TEST_CHANNEL_BUFFER_SIZE);

        let workload_instance_name = WorkloadInstanceName::builder()
            .workload_name(WORKLOAD_1_NAME)
            .build();

        runtime_mock
            .expect(vec![
                RuntimeCall::GetWorkloadId(
                    workload_instance_name.clone(),
                    Ok(WORKLOAD_ID.to_string()),
                ),
                RuntimeCall::DeleteWorkload(WORKLOAD_ID.to_string(), Ok(())),
            ])
            .await;

        let ownable_runtime_mock: Box<dyn OwnableRuntime<String, StubStateChecker>> =
            Box::new(runtime_mock.clone());
        let test_runtime_facade = Box::new(GenericRuntimeFacade::<String, StubStateChecker>::new(
            ownable_runtime_mock,
        ));

        let report_workload_states_for_workload = true;
        test_runtime_facade.delete_workload(
            workload_instance_name.clone(),
            &wl_state_sender,
            report_workload_states_for_workload,
        );

        tokio::task::yield_now().await;

        assert_execution_state_sequence(
            wl_state_receiver,
            vec![
                (
                    &workload_instance_name,
                    ExecutionState::stopping_requested(),
                ),
                (&workload_instance_name, ExecutionState::removed()),
            ],
        )
        .await;

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-delete-old-workload-without-sending-workload-states~1]
    #[tokio::test]
    async fn utest_runtime_facade_delete_workload_without_reporting_workload_states() {
        let mut runtime_mock = MockRuntimeConnector::new();

        let (wl_state_sender, mut wl_state_receiver) =
            tokio::sync::mpsc::channel(TEST_CHANNEL_BUFFER_SIZE);

        let workload_instance_name = WorkloadInstanceName::builder()
            .workload_name(WORKLOAD_1_NAME)
            .build();

        runtime_mock
            .expect(vec![
                RuntimeCall::GetWorkloadId(
                    workload_instance_name.clone(),
                    Ok(WORKLOAD_ID.to_string()),
                ),
                RuntimeCall::DeleteWorkload(WORKLOAD_ID.to_string(), Ok(())),
            ])
            .await;

        let ownable_runtime_mock: Box<dyn OwnableRuntime<String, StubStateChecker>> =
            Box::new(runtime_mock.clone());
        let test_runtime_facade = Box::new(GenericRuntimeFacade::<String, StubStateChecker>::new(
            ownable_runtime_mock,
        ));

        let report_workload_states_for_workload = false;
        test_runtime_facade.delete_workload(
            workload_instance_name.clone(),
            &wl_state_sender,
            report_workload_states_for_workload,
        );

        tokio::task::yield_now().await;

        assert!(wl_state_receiver.try_recv().is_err());

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-delete-old-workload~2]
    #[tokio::test]
    async fn utest_runtime_facade_delete_workload_failed() {
        let mut runtime_mock = MockRuntimeConnector::new();

        let (wl_state_sender, wl_state_receiver) =
            tokio::sync::mpsc::channel(TEST_CHANNEL_BUFFER_SIZE);

        let workload_instance_name = WorkloadInstanceName::builder()
            .workload_name(WORKLOAD_1_NAME)
            .build();

        runtime_mock
            .expect(vec![
                RuntimeCall::GetWorkloadId(
                    workload_instance_name.clone(),
                    Ok(WORKLOAD_ID.to_string()),
                ),
                RuntimeCall::DeleteWorkload(
                    WORKLOAD_ID.to_string(),
                    Err(crate::runtime_connectors::RuntimeError::Delete(
                        "delete failed".to_owned(),
                    )),
                ),
            ])
            .await;

        let ownable_runtime_mock: Box<dyn OwnableRuntime<String, StubStateChecker>> =
            Box::new(runtime_mock.clone());
        let test_runtime_facade = Box::new(GenericRuntimeFacade::<String, StubStateChecker>::new(
            ownable_runtime_mock,
        ));

        let report_workload_states_for_workload = true;
        test_runtime_facade.delete_workload(
            workload_instance_name.clone(),
            &wl_state_sender,
            report_workload_states_for_workload,
        );

        tokio::task::yield_now().await;

        assert_execution_state_sequence(
            wl_state_receiver,
            vec![
                (
                    &workload_instance_name,
                    ExecutionState::stopping_requested(),
                ),
                (
                    &workload_instance_name,
                    ExecutionState::delete_failed("delete failed".to_owned()),
                ),
            ],
        )
        .await;

        runtime_mock.assert_all_expectations().await;
    }
}
