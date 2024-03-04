use async_trait::async_trait;
use common::objects::{AgentName, ExecutionState, WorkloadInstanceName, WorkloadSpec};
#[cfg(test)]
use mockall::automock;

#[cfg_attr(test, mockall_double::double)]
use crate::control_interface::PipesChannelContext;

use crate::{
    runtime_connectors::{OwnableRuntime, RuntimeError, StateChecker},
    workload_state::{WorkloadStateSender, WorkloadStateSenderInterface},
};

use crate::workload::workload_control_loop::WorkloadControlLoop;
#[cfg_attr(test, mockall_double::double)]
use crate::workload::Workload;
use crate::workload::WorkloadCommandSender;
use crate::workload::{ControlLoopState, RestartCounter};

#[async_trait]
#[cfg_attr(test, automock)]
pub trait RuntimeFacade: Send + Sync + 'static {
    async fn get_reusable_running_workloads(
        &self,
        agent_name: &AgentName,
    ) -> Result<Vec<WorkloadInstanceName>, RuntimeError>;

    fn create_workload(
        &self,
        runtime_workload: WorkloadSpec,
        control_interface: Option<PipesChannelContext>,
        update_state_tx: &WorkloadStateSender,
    ) -> Workload;

    fn resume_workload(
        &self,
        runtime_workload: WorkloadSpec,
        control_interface: Option<PipesChannelContext>,
        update_state_tx: &WorkloadStateSender,
    ) -> Workload;

    fn delete_workload(
        &self,
        instance_name: WorkloadInstanceName,
        update_state_tx: &WorkloadStateSender,
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
    async fn get_reusable_running_workloads(
        &self,
        agent_name: &AgentName,
    ) -> Result<Vec<WorkloadInstanceName>, RuntimeError> {
        log::debug!(
            "Searching for reusable '{}' workloads on agent '{}'.",
            self.runtime.name(),
            agent_name
        );
        self.runtime.get_reusable_workloads(agent_name).await
    }

    // [impl->swdd~agent-create-workload~1]
    fn create_workload(
        &self,
        workload_spec: WorkloadSpec,
        control_interface: Option<PipesChannelContext>,
        update_state_tx: &WorkloadStateSender,
    ) -> Workload {
        let runtime = self.runtime.to_owned();
        let update_state_tx = update_state_tx.clone();
        let control_interface_path = control_interface
            .as_ref()
            .map(|control_interface| control_interface.get_api_location());

        let workload_name = workload_spec.instance_name.workload_name().to_owned();
        log::info!(
            "Creating '{}' workload '{}'.",
            runtime.name(),
            workload_name,
        );
        let (workload_channel_sender, command_receiver) = WorkloadCommandSender::new();
        let workload_channel = workload_channel_sender.clone();
        tokio::spawn(async move {
            let instance_name = workload_spec.instance_name.clone();
            workload_channel
                .create(workload_spec, control_interface_path)
                .await
                .unwrap_or_else(|err| {
                    log::warn!("Failed to send restart workload command: '{}'", err);
                });

            let control_loop_state = ControlLoopState {
                instance_name,
                workload_id: None,
                state_checker: None,
                update_state_tx,
                runtime,
                command_receiver,
                workload_channel,
                restart_counter: RestartCounter::new(),
            };

            WorkloadControlLoop::run(control_loop_state).await;
        });

        Workload::new(workload_name, workload_channel_sender, control_interface)
    }

    // [impl->swdd~agent-resume-workload~1]
    fn resume_workload(
        &self,
        workload_spec: WorkloadSpec,
        control_interface: Option<PipesChannelContext>,
        update_state_tx: &WorkloadStateSender,
    ) -> Workload {
        let workload_name = workload_spec.instance_name.workload_name().to_owned();
        let runtime = self.runtime.to_owned();
        let update_state_tx = update_state_tx.clone();

        log::info!(
            "Resuming '{}' workload '{}'.",
            runtime.name(),
            workload_name,
        );

        let (workload_channel, command_receiver) = WorkloadCommandSender::new();
        let workload_channel_retry = workload_channel.clone();
        tokio::spawn(async move {
            let instance_name = workload_spec.instance_name.clone();
            let workload_name = instance_name.workload_name();
            let workload_id = runtime.get_workload_id(&workload_spec.instance_name).await;

            let state_checker: Option<StChecker> = match workload_id.as_ref() {
                Ok(wl_id) => runtime
                    .start_checker(wl_id, workload_spec, update_state_tx.clone())
                    .await
                    .map_err(|err| {
                        log::warn!(
                            "Failed to start state checker when resuming workload '{}': '{}'",
                            workload_name,
                            err
                        );
                        err
                    })
                    .ok(),
                Err(err) => {
                    log::warn!(
                        "Failed to get workload id when resuming workload '{}': '{}'",
                        workload_name,
                        err
                    );
                    None
                }
            };

            let control_loop_state = ControlLoopState {
                instance_name,
                workload_id: workload_id.ok(),
                state_checker,
                update_state_tx,
                runtime,
                command_receiver,
                workload_channel: workload_channel_retry,
                restart_counter: RestartCounter::new(),
            };

            WorkloadControlLoop::run(control_loop_state).await;
        });

        Workload::new(workload_name, workload_channel, control_interface)
    }

    // [impl->swdd~agent-delete-old-workload~1]
    fn delete_workload(
        &self,
        instance_name: WorkloadInstanceName,
        update_state_tx: &WorkloadStateSender,
    ) {
        let runtime = self.runtime.to_owned();
        let update_state_tx = update_state_tx.clone();

        log::info!(
            "Deleting '{}' workload '{}' on agent '{}'",
            runtime.name(),
            instance_name.workload_name(),
            instance_name.agent_name(),
        );

        tokio::spawn(async move {
            update_state_tx
                .report_workload_execution_state(
                    &instance_name,
                    ExecutionState::stopping_triggered(),
                )
                .await;

            if let Ok(id) = runtime.get_workload_id(&instance_name).await {
                if let Err(err) = runtime.delete_workload(&id).await {
                    update_state_tx
                        .report_workload_execution_state(
                            &instance_name,
                            ExecutionState::delete_failed(err),
                        )
                        .await;

                    return; // The early exit is needed to skip sending the removed message.
                }
            } else {
                log::debug!("Workload '{}' already gone.", instance_name);
            }

            update_state_tx
                .report_workload_execution_state(&instance_name, ExecutionState::removed())
                .await;
        });
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
        generate_test_workload_spec_with_param, ExecutionState, WorkloadInstanceName,
    };

    use crate::{
        control_interface::MockPipesChannelContext,
        runtime_connectors::{
            runtime_connector::test::{MockRuntimeConnector, RuntimeCall, StubStateChecker},
            GenericRuntimeFacade, OwnableRuntime, RuntimeFacade,
        },
        workload::MockWorkload,
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

        runtime_mock
            .expect(vec![RuntimeCall::GetReusableWorkloads(
                AGENT_NAME.into(),
                Ok(vec![workload_instance_name.clone()]),
            )])
            .await;

        let ownable_runtime_mock: Box<dyn OwnableRuntime<String, StubStateChecker>> =
            Box::new(runtime_mock.clone());
        let test_runtime_facade = Box::new(GenericRuntimeFacade::<String, StubStateChecker>::new(
            ownable_runtime_mock,
        ));

        assert_eq!(
            test_runtime_facade
                .get_reusable_running_workloads(&AGENT_NAME.into())
                .await
                .unwrap(),
            vec![workload_instance_name]
        );

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-create-workload~1]
    #[tokio::test]
    async fn utest_runtime_facade_create_workload() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let mut control_interface_mock = MockPipesChannelContext::default();
        control_interface_mock
            .expect_get_api_location()
            .once()
            .return_const(PIPES_LOCATION);

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let (wl_state_sender, _wl_state_receiver) =
            tokio::sync::mpsc::channel(TEST_CHANNEL_BUFFER_SIZE);

        let mock_workload = MockWorkload::default();
        let new_workload_context = MockWorkload::new_context();
        new_workload_context
            .expect()
            .once()
            .return_once(|_, _, _| mock_workload);

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock
            .expect(vec![RuntimeCall::CreateWorkload(
                workload_spec.clone(),
                Some(PIPES_LOCATION.into()),
                wl_state_sender.clone(),
                Ok((WORKLOAD_ID.to_string(), StubStateChecker::new())),
            )])
            .await;

        let ownable_runtime_mock: Box<dyn OwnableRuntime<String, StubStateChecker>> =
            Box::new(runtime_mock.clone());
        let test_runtime_facade = Box::new(GenericRuntimeFacade::<String, StubStateChecker>::new(
            ownable_runtime_mock,
        ));

        let _workload = test_runtime_facade.create_workload(
            workload_spec.clone(),
            Some(control_interface_mock),
            &wl_state_sender,
        );

        tokio::task::yield_now().await;

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-resume-workload~1]
    #[tokio::test]
    async fn utest_runtime_facade_resume_workload() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let control_interface_mock = MockPipesChannelContext::default();

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let (wl_state_sender, _wl_state_receiver) =
            tokio::sync::mpsc::channel(TEST_CHANNEL_BUFFER_SIZE);

        let mock_workload = MockWorkload::default();
        let new_workload_context = MockWorkload::new_context();
        new_workload_context
            .expect()
            .once()
            .return_once(|_, _, _| mock_workload);

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock
            .expect(vec![
                RuntimeCall::GetWorkloadId(
                    workload_spec.instance_name.clone(),
                    Ok(WORKLOAD_ID.to_string()),
                ),
                RuntimeCall::StartChecker(
                    WORKLOAD_ID.to_string(),
                    workload_spec.clone(),
                    wl_state_sender.clone(),
                    Ok(StubStateChecker::new()),
                ),
            ])
            .await;

        let ownable_runtime_mock: Box<dyn OwnableRuntime<String, StubStateChecker>> =
            Box::new(runtime_mock.clone());
        let test_runtime_facade = Box::new(GenericRuntimeFacade::<String, StubStateChecker>::new(
            ownable_runtime_mock,
        ));

        let _workload = test_runtime_facade.resume_workload(
            workload_spec.clone(),
            Some(control_interface_mock),
            &wl_state_sender,
        );

        tokio::task::yield_now().await;

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-resume-workload~1]
    #[tokio::test]
    async fn utest_runtime_facade_resume_workload_list_fails() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let control_interface_mock = MockPipesChannelContext::default();

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let (wl_state_sender, _wl_state_receiver) =
            tokio::sync::mpsc::channel(TEST_CHANNEL_BUFFER_SIZE);

        let mock_workload = MockWorkload::default();
        let new_workload_context = MockWorkload::new_context();
        new_workload_context
            .expect()
            .once()
            .return_once(|_, _, _| mock_workload);

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock
            .expect(vec![RuntimeCall::GetWorkloadId(
                workload_spec.instance_name.clone(),
                Err(crate::runtime_connectors::RuntimeError::List(
                    "some list workload error".to_string(),
                )),
            )])
            .await;

        let ownable_runtime_mock: Box<dyn OwnableRuntime<String, StubStateChecker>> =
            Box::new(runtime_mock.clone());
        let test_runtime_facade = Box::new(GenericRuntimeFacade::<String, StubStateChecker>::new(
            ownable_runtime_mock,
        ));

        let _workload = test_runtime_facade.resume_workload(
            workload_spec.clone(),
            Some(control_interface_mock),
            &wl_state_sender,
        );

        tokio::task::yield_now().await;

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-resume-workload~1]
    #[tokio::test]
    async fn utest_runtime_facade_resume_workload_start_state_checker_fails() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let control_interface_mock = MockPipesChannelContext::default();

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let (wl_state_sender, _wl_state_receiver) =
            tokio::sync::mpsc::channel(TEST_CHANNEL_BUFFER_SIZE);

        let mock_workload = MockWorkload::default();
        let new_workload_context = MockWorkload::new_context();
        new_workload_context
            .expect()
            .once()
            .return_once(|_, _, _| mock_workload);

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock
            .expect(vec![
                RuntimeCall::GetWorkloadId(
                    workload_spec.instance_name.clone(),
                    Ok(WORKLOAD_ID.to_string()),
                ),
                RuntimeCall::StartChecker(
                    WORKLOAD_ID.to_string(),
                    workload_spec.clone(),
                    wl_state_sender.clone(),
                    Err(crate::runtime_connectors::RuntimeError::Create(
                        "some state checker error".to_string(),
                    )),
                ),
            ])
            .await;

        let ownable_runtime_mock: Box<dyn OwnableRuntime<String, StubStateChecker>> =
            Box::new(runtime_mock.clone());
        let test_runtime_facade = Box::new(GenericRuntimeFacade::<String, StubStateChecker>::new(
            ownable_runtime_mock,
        ));

        let _workload = test_runtime_facade.resume_workload(
            workload_spec.clone(),
            Some(control_interface_mock),
            &wl_state_sender,
        );

        tokio::task::yield_now().await;

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-delete-old-workload~1]
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

        test_runtime_facade.delete_workload(workload_instance_name, &wl_state_sender);

        tokio::task::yield_now().await;

        assert_execution_state_sequence(
            wl_state_receiver,
            vec![
                ExecutionState::stopping_triggered(),
                ExecutionState::removed(),
            ],
        )
        .await;

        runtime_mock.assert_all_expectations().await;
    }
}
