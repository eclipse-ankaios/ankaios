use std::{fmt::Display, path::PathBuf};

#[cfg_attr(test, mockall_double::double)]
use crate::control_interface::PipesChannelContext;
use crate::{runtime::Runtime, state_checker::StateChecker};
use common::{
    commands::CompleteState,
    execution_interface::ExecutionCommand,
    objects::{ExecutionState, WorkloadSpec},
    state_change_interface::{StateChangeInterface, StateChangeSender},
    std_extensions::IllegalStateResult,
};

#[cfg(test)]
use mockall::automock;

use tokio::sync::mpsc;

#[derive(Debug, PartialEq, Eq)]
pub enum WorkloadError {
    Communication(String),
    CompleteState(String),
}

impl Display for WorkloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorkloadError::Communication(msg) => {
                write!(f, "Could not send command to workload task: '{}'", msg)
            }
            WorkloadError::CompleteState(msg) => {
                write!(f, "Could not forward complete state '{}'", msg)
            }
        }
    }
}

#[derive(Debug)]
pub enum WorkloadCommand {
    Delete,
    Update(Box<WorkloadSpec>, Option<PathBuf>),
}

// #[derive(Debug)]
pub struct Workload {
    channel: mpsc::Sender<WorkloadCommand>,
    control_interface: Option<PipesChannelContext>,
}

#[cfg_attr(test, automock)]
impl Workload {
    pub fn new(
        channel: mpsc::Sender<WorkloadCommand>,
        control_interface: Option<PipesChannelContext>,
    ) -> Self {
        Workload {
            channel,
            control_interface,
        }
    }

    // [impl->swdd~agent-workload-obj-update-command~1]
    pub async fn update(
        &mut self,
        spec: WorkloadSpec,
        control_interface: Option<PipesChannelContext>,
    ) -> Result<(), WorkloadError> {
        if let Some(control_interface) = self.control_interface.take() {
            control_interface.abort_pipes_channel_task()
        }
        self.control_interface = control_interface;

        let control_interface_path = self
            .control_interface
            .as_ref()
            .map(|control_interface| control_interface.get_api_location());

        self.channel
            .send(WorkloadCommand::Update(
                Box::new(spec),
                control_interface_path,
            ))
            .await
            .map_err(|err| WorkloadError::Communication(err.to_string()))
    }

    // [impl->swdd~agent-workload-obj-delete-command~1]
    pub async fn delete(self) -> Result<(), WorkloadError> {
        if let Some(control_interface) = self.control_interface {
            control_interface.abort_pipes_channel_task()
        }

        self.channel
            .send(WorkloadCommand::Delete)
            .await
            .map_err(|err| WorkloadError::Communication(err.to_string()))
    }

    pub async fn await_new_command<WorkloadId, StChecker>(
        workload_name: String,
        agent_name: String,
        mut workload_id: Option<WorkloadId>,
        mut state_checker: Option<StChecker>,
        update_state_tx: StateChangeSender,
        runtime: Box<dyn Runtime<WorkloadId, StChecker>>,
        mut command_receiver: mpsc::Receiver<WorkloadCommand>,
    ) where
        WorkloadId: Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
    {
        loop {
            match command_receiver.recv().await {
                // [impl->swdd~agent-workload-tasks-executes-delete~1]
                Some(WorkloadCommand::Delete) => {
                    if let Some(old_id) = workload_id.take() {
                        if let Err(err) = runtime.delete_workload(&old_id).await {
                            log::warn!("Could not stop workload '{}': '{}'", workload_name, err);
                            workload_id = Some(old_id);
                        } else {
                            if let Some(old_checker) = state_checker.take() {
                                old_checker.stop_checker().await;
                            }

                            // Successfully stopped the workload and the state checker. Send a removed on the channel
                            update_state_tx
                                .update_workload_state(vec![common::objects::WorkloadState {
                                    agent_name,
                                    workload_name,
                                    execution_state: ExecutionState::ExecRemoved,
                                }])
                                .await
                                .unwrap_or_illegal_state();

                            log::debug!("Stop workload complete");
                            return;
                        }
                    } else {
                        log::debug!("Workload '{}' already gone.", workload_name);
                        return;
                    }
                }
                // [impl->swdd~agent-workload-tasks-executes-update~1]
                Some(WorkloadCommand::Update(runtime_workload_config, control_interface_path)) => {
                    if let Some(old_id) = workload_id.take() {
                        if let Err(err) = runtime.delete_workload(&old_id).await {
                            log::warn!("Could not update workload '{}': '{}'", workload_name, err);
                            workload_id = Some(old_id);
                            continue;
                        } else if let Some(old_checker) = state_checker.take() {
                            old_checker.stop_checker().await;
                        }
                    } else {
                        log::debug!("Workload '{}' already gone.", workload_name);
                    }

                    match runtime
                        .create_workload(
                            *runtime_workload_config,
                            control_interface_path,
                            update_state_tx.clone(),
                        )
                        .await
                    {
                        Ok((new_workload_id, new_state_checker)) => {
                            workload_id = Some(new_workload_id);
                            state_checker = Some(new_state_checker);
                        }
                        Err(err) => {
                            log::warn!(
                                "Could not start updated workload '{}': '{}'",
                                workload_name,
                                err
                            )
                        }
                    }

                    log::debug!("Update workload complete");
                }
                _ => {
                    log::warn!(
                        "Could not wait for internal stop command for workload '{}'.",
                        workload_name,
                    );
                    return;
                }
            }
        }
    }

    // [impl->swdd~agent-forward-responses-to-control-interface-pipe~1]
    pub async fn send_complete_state(
        &mut self,
        complete_state: CompleteState,
    ) -> Result<(), WorkloadError> {
        let control_interface =
            self.control_interface
                .as_ref()
                .ok_or(WorkloadError::CompleteState(
                    "control interface not available".to_string(),
                ))?;
        control_interface
            .get_input_pipe_sender()
            .send(ExecutionCommand::CompleteState(Box::new(complete_state)))
            .await
            .map_err(|err| WorkloadError::CompleteState(err.to_string()))
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

    use std::path::PathBuf;
    use std::time::Duration;

    use common::{
        commands::{CompleteState, UpdateWorkloadState},
        execution_interface::ExecutionCommand,
        objects::{ExecutionState, WorkloadState},
        state_change_interface::StateChangeCommand,
        test_utils::{generate_test_complete_state, generate_test_workload_spec_with_param},
    };
    use tokio::{sync::mpsc, time::timeout};

    use crate::{
        control_interface::MockPipesChannelContext,
        runtime::test::{MockRuntime, RuntimeCall, StubStateChecker},
        workload::{Workload, WorkloadCommand, WorkloadError},
    };

    const RUNTIME_NAME: &str = "runtime1";
    const AGENT_NAME: &str = "agent_x";
    const WORKLOAD_1_NAME: &str = "workload1";
    const WORKLOAD_ID: &str = "workload_id_1";
    const PIPES_LOCATION: &str = "/some/path";
    const OLD_WORKLOAD_ID: &str = "old_workload_id";
    const REQUEST_ID: &str = "request_id";

    const TEST_WL_COMMAND_BUFFER_SIZE: usize = 5;
    const TEST_EXEC_COMMAND_BUFFER_SIZE: usize = 5;

    // [utest->swdd~agent-workload-obj-update-command~1]
    #[tokio::test]
    async fn utest_workload_obj_update_success() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_tx, mut workload_command_rx) =
            mpsc::channel(TEST_WL_COMMAND_BUFFER_SIZE);

        let mut old_control_interface_mock = MockPipesChannelContext::default();
        old_control_interface_mock
            .expect_abort_pipes_channel_task()
            .once()
            .return_const(());

        let mut new_control_interface_mock = MockPipesChannelContext::default();
        new_control_interface_mock
            .expect_get_api_location()
            .once()
            .return_const(PIPES_LOCATION);

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let mut test_workload =
            Workload::new(workload_command_tx, Some(old_control_interface_mock));

        test_workload
            .update(workload_spec.clone(), Some(new_control_interface_mock))
            .await
            .unwrap();

        let expected_workload_spec = Box::new(workload_spec);
        let expected_pipes_path_buf = PathBuf::from(PIPES_LOCATION);

        assert!(matches!(
            timeout(Duration::from_millis(200), workload_command_rx.recv()).await,
            Ok(Some(WorkloadCommand::Update(
                boxed_workload_spec,
                Some(pipes_path_buf)
            )))
        if expected_workload_spec == boxed_workload_spec && expected_pipes_path_buf == pipes_path_buf));
    }

    // [utest->swdd~agent-workload-obj-update-command~1]
    #[tokio::test]
    async fn utest_workload_obj_update_error() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_tx, workload_command_rx) = mpsc::channel(TEST_WL_COMMAND_BUFFER_SIZE);

        // drop the receiver so that the send command fails
        drop(workload_command_rx);

        let mut old_control_interface_mock = MockPipesChannelContext::default();
        old_control_interface_mock
            .expect_abort_pipes_channel_task()
            .once()
            .return_const(());

        let mut new_control_interface_mock = MockPipesChannelContext::default();
        new_control_interface_mock
            .expect_get_api_location()
            .once()
            .return_const(PIPES_LOCATION);

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let mut test_workload =
            Workload::new(workload_command_tx, Some(old_control_interface_mock));

        assert!(matches!(
            test_workload
                .update(workload_spec.clone(), Some(new_control_interface_mock))
                .await,
            Err(WorkloadError::Communication(_))
        ));
    }

    // [utest->swdd~agent-workload-obj-delete-command~1]
    #[tokio::test]
    async fn utest_workload_obj_delete_success() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_tx, mut workload_command_rx) =
            mpsc::channel(TEST_WL_COMMAND_BUFFER_SIZE);

        let mut old_control_interface_mock = MockPipesChannelContext::default();
        old_control_interface_mock
            .expect_abort_pipes_channel_task()
            .once()
            .return_const(());

        let test_workload = Workload::new(workload_command_tx, Some(old_control_interface_mock));

        test_workload.delete().await.unwrap();

        assert!(matches!(
            timeout(Duration::from_millis(200), workload_command_rx.recv()).await,
            Ok(Some(WorkloadCommand::Delete))
        ));
    }

    // [utest->swdd~agent-workload-obj-delete-command~1]
    #[tokio::test]
    async fn utest_workload_obj_delete_error() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_tx, workload_command_rx) = mpsc::channel(TEST_WL_COMMAND_BUFFER_SIZE);

        // drop the receiver so that the send command fails
        drop(workload_command_rx);

        let mut old_control_interface_mock = MockPipesChannelContext::default();
        old_control_interface_mock
            .expect_abort_pipes_channel_task()
            .once()
            .return_const(());

        let test_workload = Workload::new(workload_command_tx, Some(old_control_interface_mock));

        assert!(matches!(
            test_workload.delete().await,
            Err(WorkloadError::Communication(_))
        ));
    }

    // Unfortunately this test also executes a delete of the newly updated workload. 
    // We could not avoid this as it is the only possibility to check the internal variables
    // and to properly stop the control loop in the await new command method
    // [utest->swdd~agent-workload-tasks-executes-update~1]
    #[tokio::test]
    async fn utest_workload_obj_await_new_command_update_success() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_tx, workload_command_rx) = mpsc::channel(TEST_WL_COMMAND_BUFFER_SIZE);
        let (state_change_tx, mut state_change_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let mut old_mock_state_checker = StubStateChecker::new();
        old_mock_state_checker.panic_if_not_stopped();

        // Since we also send a delete command to exit the control loop properly, the new state
        // checker will also we stopped. This also tests if the new state checker was properly stored.
        let mut new_mock_state_checker = StubStateChecker::new();
        new_mock_state_checker.panic_if_not_stopped();

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let mut runtime_mock = MockRuntime::new();
        runtime_mock
            .expect(vec![
                RuntimeCall::DeleteWorkload(OLD_WORKLOAD_ID.to_string(), Ok(())),
                RuntimeCall::CreateWorkload(
                    workload_spec.clone(),
                    Some(PIPES_LOCATION.into()),
                    state_change_tx.clone(),
                    Ok((WORKLOAD_ID.to_string(), new_mock_state_checker)),
                ),
                // Since we also send a delete command to exit the control loop properly, the new workload
                // will also we deleted. This also tests if the new workload id was properly stored.
                RuntimeCall::DeleteWorkload(WORKLOAD_ID.to_string(), Ok(())),
            ])
            .await;

        // Send the update command now. It will be buffered until the await receives it.
        workload_command_tx
            .send(WorkloadCommand::Update(
                Box::new(workload_spec.clone()),
                Some(PIPES_LOCATION.into()),
            ))
            .await
            .unwrap();
        // Send also a delete command so that we can properly get out of the loop
        workload_command_tx
            .send(WorkloadCommand::Delete)
            .await
            .unwrap();

        assert!(timeout(
            Duration::from_millis(200),
            Workload::await_new_command(
                WORKLOAD_1_NAME.to_string(),
                AGENT_NAME.to_string(),
                Some(OLD_WORKLOAD_ID.to_string()),
                Some(old_mock_state_checker),
                state_change_tx.clone(),
                Box::new(runtime_mock.clone()),
                workload_command_rx,
            )
        )
        .await
        .is_ok());

        let expected_state = UpdateWorkloadState {
            workload_states: vec![WorkloadState {
                workload_name: WORKLOAD_1_NAME.to_string(),
                agent_name: AGENT_NAME.to_string(),
                execution_state: ExecutionState::ExecRemoved,
            }],
        };

        assert!(matches!(
            timeout(Duration::from_millis(200), state_change_rx.recv()).await,
            Ok(Some(StateChangeCommand::UpdateWorkloadState(workload_state)))
        if workload_state == expected_state));

        runtime_mock.assert_all_expectations().await;
    }

    // TODO:
    // test what happens if the workload id is None
    // test what happens if the delete fails
    // test what happens if the create fails

    // [utest->swdd~agent-workload-tasks-executes-delete~1]
    #[tokio::test]
    async fn utest_workload_obj_await_new_command_delete_success() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_tx, workload_command_rx) = mpsc::channel(TEST_WL_COMMAND_BUFFER_SIZE);
        let (state_change_tx, mut state_change_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let mut mock_state_checker = StubStateChecker::new();
        mock_state_checker.panic_if_not_stopped();

        let mut runtime_mock = MockRuntime::new();
        runtime_mock
            .expect(vec![RuntimeCall::DeleteWorkload(
                OLD_WORKLOAD_ID.to_string(),
                Ok(()),
            )])
            .await;

        // Send the delete command now. It will be buffered until the await receives it.
        workload_command_tx
            .send(WorkloadCommand::Delete)
            .await
            .unwrap();

        assert!(timeout(
            Duration::from_millis(200),
            Workload::await_new_command(
                WORKLOAD_1_NAME.to_string(),
                AGENT_NAME.to_string(),
                Some(OLD_WORKLOAD_ID.to_string()),
                Some(mock_state_checker),
                state_change_tx.clone(),
                Box::new(runtime_mock.clone()),
                workload_command_rx,
            )
        )
        .await
        .is_ok());

        let expected_state = UpdateWorkloadState {
            workload_states: vec![WorkloadState {
                workload_name: WORKLOAD_1_NAME.to_string(),
                agent_name: AGENT_NAME.to_string(),
                execution_state: ExecutionState::ExecRemoved,
            }],
        };

        assert!(matches!(
            timeout(Duration::from_millis(200), state_change_rx.recv()).await,
            Ok(Some(StateChangeCommand::UpdateWorkloadState(workload_state)))
        if workload_state == expected_state));

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-tasks-executes-delete~1]
    #[tokio::test]
    async fn utest_workload_obj_await_new_command_delete_first_fail_second_success() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_tx, workload_command_rx) = mpsc::channel(TEST_WL_COMMAND_BUFFER_SIZE);
        let (state_change_tx, mut state_change_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let mut mock_state_checker = StubStateChecker::new();
        mock_state_checker.panic_if_not_stopped();

        let mut runtime_mock = MockRuntime::new();
        runtime_mock
            .expect(vec![
                RuntimeCall::DeleteWorkload(
                    OLD_WORKLOAD_ID.to_string(),
                    Err(crate::runtime::RuntimeError::Delete(
                        "some delete error".to_string(),
                    )),
                ),
                // First fail, now success
                RuntimeCall::DeleteWorkload(OLD_WORKLOAD_ID.to_string(), Ok(())),
            ])
            .await;

        // Send the delete command now. It will be buffered until the await receives it.
        workload_command_tx
            .send(WorkloadCommand::Delete)
            .await
            .unwrap();
        workload_command_tx
            .send(WorkloadCommand::Delete)
            .await
            .unwrap();

        assert!(timeout(
            Duration::from_millis(200),
            Workload::await_new_command(
                WORKLOAD_1_NAME.to_string(),
                AGENT_NAME.to_string(),
                Some(OLD_WORKLOAD_ID.to_string()),
                Some(mock_state_checker),
                state_change_tx.clone(),
                Box::new(runtime_mock.clone()),
                workload_command_rx,
            )
        )
        .await
        .is_ok());

        let expected_state = UpdateWorkloadState {
            workload_states: vec![WorkloadState {
                workload_name: WORKLOAD_1_NAME.to_string(),
                agent_name: AGENT_NAME.to_string(),
                execution_state: ExecutionState::ExecRemoved,
            }],
        };

        assert!(matches!(
            timeout(Duration::from_millis(200), state_change_rx.recv()).await,
            Ok(Some(StateChangeCommand::UpdateWorkloadState(workload_state)))
        if workload_state == expected_state));

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-tasks-executes-delete~1]
    #[tokio::test]
    async fn utest_workload_obj_await_new_command_delete_already_gone() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_tx, workload_command_rx) = mpsc::channel(TEST_WL_COMMAND_BUFFER_SIZE);
        let (state_change_tx, _state_change_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let runtime_mock = MockRuntime::new();

        // Send the delete command now. It will be buffered until the await receives it.
        workload_command_tx
            .send(WorkloadCommand::Delete)
            .await
            .unwrap();

        assert!(timeout(
            Duration::from_millis(200),
            Workload::await_new_command(
                WORKLOAD_1_NAME.to_string(),
                AGENT_NAME.to_string(),
                None,
                None,
                state_change_tx.clone(),
                Box::new(runtime_mock.clone()),
                workload_command_rx,
            )
        )
        .await
        .is_ok());

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-forward-responses-to-control-interface-pipe~1]
    #[tokio::test]
    async fn utest_workload_obj_send_complete_state_success() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_tx, _workload_command_rx) =
            mpsc::channel(TEST_WL_COMMAND_BUFFER_SIZE);
        let (state_change_tx, mut state_change_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let mut control_interface_mock = MockPipesChannelContext::default();
        control_interface_mock
            .expect_get_input_pipe_sender()
            .once()
            .return_const(state_change_tx);

        let mut test_workload = Workload::new(workload_command_tx, Some(control_interface_mock));
        let complete_state = generate_test_complete_state(
            format!("{WORKLOAD_1_NAME}@{REQUEST_ID}"),
            vec![generate_test_workload_spec_with_param(
                AGENT_NAME.to_string(),
                WORKLOAD_1_NAME.to_string(),
                RUNTIME_NAME.to_string(),
            )],
        );

        test_workload
            .send_complete_state(complete_state.clone())
            .await
            .unwrap();

        let expected_complete_state_box = Box::new(complete_state);

        assert!(matches!(
            timeout(Duration::from_millis(200), state_change_rx.recv()).await,
            Ok(Some(ExecutionCommand::CompleteState(complete_state_box)))
        if expected_complete_state_box == complete_state_box));
    }

    // [utest->swdd~agent-forward-responses-to-control-interface-pipe~1]
    #[tokio::test]
    async fn utest_workload_obj_send_complete_state_pipes_communication_error() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_tx, _workload_command_rx) =
            mpsc::channel(TEST_WL_COMMAND_BUFFER_SIZE);
        let (state_change_tx, state_change_rx) = mpsc::channel(TEST_WL_COMMAND_BUFFER_SIZE);

        drop(state_change_rx);

        let mut control_interface_mock = MockPipesChannelContext::default();
        control_interface_mock
            .expect_get_input_pipe_sender()
            .once()
            .return_const(state_change_tx);

        let mut test_workload = Workload::new(workload_command_tx, Some(control_interface_mock));
        let complete_state = CompleteState::default();

        assert!(matches!(
            test_workload.send_complete_state(complete_state).await,
            Err(WorkloadError::CompleteState(_))
        ));
    }

    // [utest->swdd~agent-forward-responses-to-control-interface-pipe~1]
    #[tokio::test]
    async fn utest_workload_obj_send_complete_state_no_control_interface() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_tx, _workload_command_rx) =
            mpsc::channel(TEST_WL_COMMAND_BUFFER_SIZE);

        let mut test_workload = Workload::new(workload_command_tx, None);
        let complete_state = CompleteState::default();

        assert!(matches!(
            test_workload.send_complete_state(complete_state).await,
            Err(WorkloadError::CompleteState(_))
        ));
    }
}
