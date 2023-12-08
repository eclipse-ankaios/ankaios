use async_trait::async_trait;
use common::{
    objects::{AgentName, WorkloadExecutionInstanceName, WorkloadInstanceName, WorkloadSpec},
    state_change_interface::StateChangeSender,
};
#[cfg(test)]
use mockall::automock;

#[cfg_attr(test, mockall_double::double)]
use crate::control_interface::PipesChannelContext;

use crate::runtime_connectors::{OwnableRuntime, RuntimeError, StateChecker};

#[cfg_attr(test, mockall_double::double)]
use crate::workload::Workload;
#[cfg_attr(test, mockall_double::double)]
use crate::workload_queue::workload_command_queue::WorkloadCommandQueue;
use crate::workload_queue::WorkloadCommandChannel;

#[async_trait]
#[cfg_attr(test, automock)]
pub trait RuntimeFacade: Send + Sync + 'static {
    async fn get_reusable_running_workloads(
        &self,
        agent_name: &AgentName,
    ) -> Result<Vec<WorkloadExecutionInstanceName>, RuntimeError>;

    fn create_workload(
        &self,
        runtime_workload: WorkloadSpec,
        control_interface: Option<PipesChannelContext>,
        update_state_tx: &StateChangeSender,
    ) -> Workload;

    fn replace_workload(
        &self,
        existing_workload_name: WorkloadExecutionInstanceName,
        new_workload_spec: WorkloadSpec,
        control_interface: Option<PipesChannelContext>,
        update_state_tx: &StateChangeSender,
    ) -> Workload;

    fn resume_workload(
        &self,
        runtime_workload: WorkloadSpec,
        control_interface: Option<PipesChannelContext>,
        update_state_tx: &StateChangeSender,
    ) -> Workload;

    fn delete_workload(&self, instance_name: WorkloadExecutionInstanceName);
}

pub struct GenericRuntimeFacade<
    WorkloadId: Send + Sync + 'static,
    StChecker: StateChecker<WorkloadId> + Send + Sync,
> {
    runtime: Box<dyn OwnableRuntime<WorkloadId, StChecker>>,
}

impl<WorkloadId, StChecker> GenericRuntimeFacade<WorkloadId, StChecker>
where
    WorkloadId: Send + Sync + 'static,
    StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
{
    pub fn new(runtime: Box<dyn OwnableRuntime<WorkloadId, StChecker>>) -> Self {
        GenericRuntimeFacade { runtime }
    }
}

#[async_trait]
impl<
        WorkloadId: Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
    > RuntimeFacade for GenericRuntimeFacade<WorkloadId, StChecker>
{
    // [impl->swdd~agent-facade-forwards-list-reusable-workloads-call~1]
    async fn get_reusable_running_workloads(
        &self,
        agent_name: &AgentName,
    ) -> Result<Vec<WorkloadExecutionInstanceName>, RuntimeError> {
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
        update_state_tx: &StateChangeSender,
    ) -> Workload {
        let workload_name = workload_spec.name.clone();
        let agent_name = workload_spec.agent.clone();
        let runtime = self.runtime.to_owned();
        let update_state_tx = update_state_tx.clone();
        let control_interface_path = control_interface
            .as_ref()
            .map(|control_interface| control_interface.get_api_location());

        log::info!(
            "Creating '{}' workload '{}' on agent '{}'",
            runtime.name(),
            workload_name,
            agent_name
        );
        let (workload_channel, command_receiver) = WorkloadCommandChannel::new();
        let workload_channel_retry = workload_channel.clone();
        tokio::spawn(async move {
            let workload_name = workload_spec.name.clone();
            let create_result = runtime
                .create_workload(
                    workload_spec.clone(),
                    control_interface_path.clone(),
                    update_state_tx.clone(),
                )
                .await;

            let (workload_id, state_checker) = if let Ok((id, checker)) = create_result {
                (Some(id), Some(checker))
            } else {
                log::warn!(
                    "Failed to create workload: '{}': '{}'",
                    workload_name,
                    create_result.err().unwrap()
                );
                workload_channel_retry
                    .restart(workload_spec, control_interface_path)
                    .await
                    .unwrap_or_else(|err| {
                        log::warn!("Failed to send restart workload command: '{}'", err);
                    });
                (None, None)
            };

            let mut workload_loop: WorkloadCommandQueue<WorkloadId, StChecker> =
                WorkloadCommandQueue::new(
                    workload_name,
                    agent_name,
                    workload_id,
                    state_checker,
                    update_state_tx,
                    runtime,
                    command_receiver,
                    workload_channel_retry,
                );
            workload_loop.await_new_command().await;
        });

        Workload::new(workload_name, workload_channel, control_interface)
    }

    // [impl->swdd~agent-replace-workload~1]
    fn replace_workload(
        &self,
        old_instance_name: WorkloadExecutionInstanceName,
        new_workload_spec: WorkloadSpec,
        control_interface: Option<PipesChannelContext>,
        update_state_tx: &StateChangeSender,
    ) -> Workload {
        let workload_name = new_workload_spec.name.clone();
        let agent_name = new_workload_spec.agent.clone();
        let runtime = self.runtime.to_owned();
        let update_state_tx = update_state_tx.clone();
        let control_interface_path = control_interface
            .as_ref()
            .map(|control_interface| control_interface.get_api_location());

        log::info!(
            "Replacing '{}' workload '{}' on agent '{}'",
            runtime.name(),
            workload_name,
            agent_name
        );

        let (workload_channel, command_receiver) = WorkloadCommandChannel::new();
        let workload_channel_retry = workload_channel.clone();
        tokio::spawn(async move {
            let workload_name = new_workload_spec.name.clone();
            match runtime.get_workload_id(&old_instance_name).await {
                Ok(old_id) => runtime
                    .delete_workload(&old_id)
                    .await
                    .unwrap_or_else(|err| {
                        log::warn!(
                            "Failed to delete workload when replacing workload '{}': '{}'",
                            workload_name,
                            err
                        )
                    }),
                Err(err) => log::warn!(
                    "Failed to get workload id when replacing workload '{}': '{}'",
                    workload_name,
                    err
                ),
            }

            let create_result = runtime
                .create_workload(
                    new_workload_spec.clone(),
                    control_interface_path.clone(),
                    update_state_tx.clone(),
                )
                .await;

            let (workload_id, state_checker) = if let Ok((id, checker)) = create_result {
                (Some(id), Some(checker))
            } else {
                log::warn!(
                    "Failed to create workload: '{}': '{}'",
                    workload_name,
                    create_result.err().unwrap()
                );
                workload_channel_retry
                    .restart(new_workload_spec, control_interface_path)
                    .await
                    .unwrap_or_else(|err| {
                        log::warn!("Failed to send restart workload command: '{}'", err);
                    });
                (None, None)
            };

            // replace workload_id and state_checker through Option directly and pass in None if create_workload fails
            let mut workload_loop: WorkloadCommandQueue<WorkloadId, StChecker> =
                WorkloadCommandQueue::new(
                    workload_name,
                    agent_name,
                    workload_id,
                    state_checker,
                    update_state_tx,
                    runtime,
                    command_receiver,
                    workload_channel_retry,
                );
            workload_loop.await_new_command().await;
        });

        Workload::new(workload_name, workload_channel, control_interface)
    }

    // [impl->swdd~agent-resume-workload~1]
    fn resume_workload(
        &self,
        workload_spec: WorkloadSpec,
        control_interface: Option<PipesChannelContext>,
        update_state_tx: &StateChangeSender,
    ) -> Workload {
        let workload_name = workload_spec.name.clone();
        let agent_name = workload_spec.agent.clone();
        let runtime = self.runtime.to_owned();
        let update_state_tx = update_state_tx.clone();

        log::info!(
            "Resuming '{}' workload '{}' on agent '{}'",
            runtime.name(),
            workload_name,
            agent_name
        );

        let (workload_channel, command_receiver) = WorkloadCommandChannel::new();
        let workload_channel_retry = workload_channel.clone();
        tokio::spawn(async move {
            let workload_name = workload_spec.name.clone();
            let workload_id = runtime
                .get_workload_id(&workload_spec.instance_name())
                .await;

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

            let mut workload_loop: WorkloadCommandQueue<WorkloadId, StChecker> =
                WorkloadCommandQueue::new(
                    workload_name,
                    agent_name,
                    workload_id.ok(),
                    state_checker,
                    update_state_tx,
                    runtime,
                    command_receiver,
                    workload_channel_retry,
                );
            workload_loop.await_new_command().await;
        });

        Workload::new(workload_name, workload_channel, control_interface)
    }

    // [impl->swdd~agent-delete-old-workload~1]
    fn delete_workload(&self, instance_name: WorkloadExecutionInstanceName) {
        let runtime = self.runtime.to_owned();

        log::info!(
            "Deleting '{}' workload '{}' on agent '{}'",
            runtime.name(),
            instance_name.workload_name(),
            instance_name.agent_name(),
        );

        tokio::spawn(async move {
            runtime
                .delete_workload(&runtime.get_workload_id(&instance_name).await?)
                .await
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

    use common::{
        objects::{WorkloadExecutionInstanceName, WorkloadInstanceName},
        state_change_interface::StateChangeCommand,
        test_utils::generate_test_workload_spec_with_param,
    };
    use mockall::predicate;
    use tokio::sync::mpsc::Sender;

    use crate::{
        control_interface::MockPipesChannelContext,
        runtime_connectors::{
            runtime_connector::test::{MockRuntimeConnector, RuntimeCall, StubStateChecker},
            OwnableRuntime,
        },
        runtime_connectors::{GenericRuntimeFacade, RuntimeFacade},
        workload::MockWorkload,
        workload_queue::workload_command_queue::MockWorkloadCommandQueue,
    };

    const RUNTIME_NAME: &str = "runtime1";
    const AGENT_NAME: &str = "agent_x";
    const WORKLOAD_1_NAME: &str = "workload1";
    const WORKLOAD_ID: &str = "workload_id_1";
    const PIPES_LOCATION: &str = "/some/path";
    const OLD_WORKLOAD_ID: &str = "old_workload_id";
    const TEST_CHANNEL_BUFFER_SIZE: usize = 20;

    // [utest->swdd~agent-facade-forwards-list-reusable-workloads-call~1]
    #[tokio::test]
    async fn utest_runtime_facade_reusable_running_workloads() {
        let mut runtime_mock = MockRuntimeConnector::new();

        let workload_instance_name = WorkloadExecutionInstanceName::builder()
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

        let (to_server, _server_receiver) =
            tokio::sync::mpsc::channel::<StateChangeCommand>(TEST_CHANNEL_BUFFER_SIZE);

        let mock_workload = MockWorkload::default();
        let new_workload_context = MockWorkload::new_context();
        new_workload_context
            .expect()
            .once()
            .return_once(|_, _, _| mock_workload);

        let to_server_clone = to_server.clone();

        let mut workload_command_queue_mock = MockWorkloadCommandQueue::default();
        workload_command_queue_mock
            .expect_await_new_command()
            .once();
        let workload_command_queue_new_context = MockWorkloadCommandQueue::new_context();
        workload_command_queue_new_context
            .expect()
            .once()
            .with(
                predicate::eq(WORKLOAD_1_NAME.to_string()),
                predicate::eq(AGENT_NAME.to_string()),
                predicate::eq(Some(WORKLOAD_ID.to_string())),
                predicate::always(),
                predicate::function(move |sender: &Sender<StateChangeCommand>| {
                    sender.same_channel(&to_server_clone)
                }),
                predicate::always(),
                predicate::always(),
                predicate::always(),
            )
            .return_once(
                |_, _, _: Option<String>, _: Option<StubStateChecker>, _, _, _, _| {
                    workload_command_queue_mock
                },
            );

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock
            .expect(vec![RuntimeCall::CreateWorkload(
                workload_spec.clone(),
                Some(PIPES_LOCATION.into()),
                to_server.clone(),
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
            &to_server,
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

        let (to_server, _server_receiver) =
            tokio::sync::mpsc::channel::<StateChangeCommand>(TEST_CHANNEL_BUFFER_SIZE);

        let mock_workload = MockWorkload::default();
        let new_workload_context = MockWorkload::new_context();
        new_workload_context
            .expect()
            .once()
            .return_once(|_, _, _| mock_workload);

        let to_server_clone = to_server.clone();

        let mut workload_command_queue_mock = MockWorkloadCommandQueue::default();
        workload_command_queue_mock
            .expect_await_new_command()
            .once();
        let workload_command_queue_new_context = MockWorkloadCommandQueue::new_context();
        workload_command_queue_new_context
            .expect()
            .once()
            .with(
                predicate::eq(WORKLOAD_1_NAME.to_string()),
                predicate::eq(AGENT_NAME.to_string()),
                predicate::eq(Some(WORKLOAD_ID.to_string())),
                predicate::always(),
                predicate::function(move |sender: &Sender<StateChangeCommand>| {
                    sender.same_channel(&to_server_clone)
                }),
                predicate::always(),
                predicate::always(),
                predicate::always(),
            )
            .return_once(
                |_, _, _: Option<String>, _: Option<StubStateChecker>, _, _, _, _| {
                    workload_command_queue_mock
                },
            );

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock
            .expect(vec![
                RuntimeCall::GetWorkloadId(
                    workload_spec.instance_name(),
                    Ok(WORKLOAD_ID.to_string()),
                ),
                RuntimeCall::StartChecker(
                    WORKLOAD_ID.to_string(),
                    workload_spec.clone(),
                    to_server.clone(),
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
            &to_server,
        );

        tokio::task::yield_now().await;

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-replace-workload~1]
    #[tokio::test]
    async fn utest_runtime_facade_replace_workload_all_success() {
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

        let (to_server, _server_receiver) =
            tokio::sync::mpsc::channel::<StateChangeCommand>(TEST_CHANNEL_BUFFER_SIZE);

        let mock_workload = MockWorkload::default();
        let new_workload_context = MockWorkload::new_context();
        new_workload_context
            .expect()
            .once()
            .return_once(|_, _, _| mock_workload);

        let to_server_clone = to_server.clone();

        let mut workload_command_queue_mock = MockWorkloadCommandQueue::default();
        workload_command_queue_mock
            .expect_await_new_command()
            .once();
        let workload_command_queue_new_context = MockWorkloadCommandQueue::new_context();
        workload_command_queue_new_context
            .expect()
            .once()
            .with(
                predicate::eq(WORKLOAD_1_NAME.to_string()),
                predicate::eq(AGENT_NAME.to_string()),
                predicate::eq(Some(WORKLOAD_ID.to_string())),
                predicate::always(),
                predicate::function(move |sender: &Sender<StateChangeCommand>| {
                    sender.same_channel(&to_server_clone)
                }),
                predicate::always(),
                predicate::always(),
                predicate::always(),
            )
            .return_once(
                |_, _, _: Option<String>, _: Option<StubStateChecker>, _, _, _, _| {
                    workload_command_queue_mock
                },
            );
        let old_workload_instance_name = WorkloadExecutionInstanceName::builder()
            .workload_name(WORKLOAD_1_NAME)
            .config(&"config".to_string())
            .build();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock
            .expect(vec![
                RuntimeCall::GetWorkloadId(
                    old_workload_instance_name.clone(),
                    Ok(OLD_WORKLOAD_ID.to_string()),
                ),
                RuntimeCall::DeleteWorkload(OLD_WORKLOAD_ID.to_string(), Ok(())),
                RuntimeCall::CreateWorkload(
                    workload_spec.clone(),
                    Some(PIPES_LOCATION.into()),
                    to_server.clone(),
                    Ok((WORKLOAD_ID.to_string(), StubStateChecker::new())),
                ),
            ])
            .await;

        let ownable_runtime_mock: Box<dyn OwnableRuntime<String, StubStateChecker>> =
            Box::new(runtime_mock.clone());
        let test_runtime_facade = Box::new(GenericRuntimeFacade::<String, StubStateChecker>::new(
            ownable_runtime_mock,
        ));

        let _workload = test_runtime_facade.replace_workload(
            old_workload_instance_name,
            workload_spec.clone(),
            Some(control_interface_mock),
            &to_server,
        );

        tokio::task::yield_now().await;

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-replace-workload~1]
    #[tokio::test]
    async fn utest_runtime_facade_replace_workload_get_id_fails() {
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

        let (to_server, _server_receiver) =
            tokio::sync::mpsc::channel::<StateChangeCommand>(TEST_CHANNEL_BUFFER_SIZE);

        let mock_workload = MockWorkload::default();
        let new_workload_context = MockWorkload::new_context();
        new_workload_context
            .expect()
            .once()
            .return_once(|_, _, _| mock_workload);

        let to_server_clone = to_server.clone();

        let mut workload_command_queue_mock = MockWorkloadCommandQueue::default();
        workload_command_queue_mock
            .expect_await_new_command()
            .once();
        let workload_command_queue_new_context = MockWorkloadCommandQueue::new_context();
        workload_command_queue_new_context
            .expect()
            .once()
            .with(
                predicate::eq(WORKLOAD_1_NAME.to_string()),
                predicate::eq(AGENT_NAME.to_string()),
                predicate::eq(Some(WORKLOAD_ID.to_string())),
                predicate::always(),
                predicate::function(move |sender: &Sender<StateChangeCommand>| {
                    sender.same_channel(&to_server_clone)
                }),
                predicate::always(),
                predicate::always(),
                predicate::always(),
            )
            .return_once(
                |_, _, _: Option<String>, _: Option<StubStateChecker>, _, _, _, _| {
                    workload_command_queue_mock
                },
            );

        let old_workload_instance_name = WorkloadExecutionInstanceName::builder()
            .workload_name(WORKLOAD_1_NAME)
            .config(&"config".to_string())
            .build();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock
            .expect(vec![
                RuntimeCall::GetWorkloadId(
                    old_workload_instance_name.clone(),
                    Err(crate::runtime_connectors::RuntimeError::List(
                        "some error".to_string(),
                    )),
                ),
                // The expectation is that delete is not called as the workload is now gone
                RuntimeCall::CreateWorkload(
                    workload_spec.clone(),
                    Some(PIPES_LOCATION.into()),
                    to_server.clone(),
                    Ok((WORKLOAD_ID.to_string(), StubStateChecker::new())),
                ),
            ])
            .await;

        let ownable_runtime_mock: Box<dyn OwnableRuntime<String, StubStateChecker>> =
            Box::new(runtime_mock.clone());
        let test_runtime_facade = Box::new(GenericRuntimeFacade::<String, StubStateChecker>::new(
            ownable_runtime_mock,
        ));

        let _workload = test_runtime_facade.replace_workload(
            old_workload_instance_name,
            workload_spec.clone(),
            Some(control_interface_mock),
            &to_server,
        );

        tokio::task::yield_now().await;

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-replace-workload~1]
    #[tokio::test]
    async fn utest_runtime_facade_replace_workload_delete_fails_create_still_called() {
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

        let (to_server, _server_receiver) =
            tokio::sync::mpsc::channel::<StateChangeCommand>(TEST_CHANNEL_BUFFER_SIZE);

        let mock_workload = MockWorkload::default();
        let new_workload_context = MockWorkload::new_context();
        new_workload_context
            .expect()
            .once()
            .return_once(|_, _, _| mock_workload);

        let to_server_clone = to_server.clone();

        let mut workload_command_queue_mock = MockWorkloadCommandQueue::default();
        workload_command_queue_mock
            .expect_await_new_command()
            .once();
        let workload_command_queue_new_context = MockWorkloadCommandQueue::new_context();
        workload_command_queue_new_context
            .expect()
            .once()
            .with(
                predicate::eq(WORKLOAD_1_NAME.to_string()),
                predicate::eq(AGENT_NAME.to_string()),
                predicate::eq(Some(WORKLOAD_ID.to_string())),
                predicate::always(),
                predicate::function(move |sender: &Sender<StateChangeCommand>| {
                    sender.same_channel(&to_server_clone)
                }),
                predicate::always(),
                predicate::always(),
                predicate::always(),
            )
            .return_once(
                |_, _, _: Option<String>, _: Option<StubStateChecker>, _, _, _, _| {
                    workload_command_queue_mock
                },
            );

        let old_workload_instance_name = WorkloadExecutionInstanceName::builder()
            .workload_name(WORKLOAD_1_NAME)
            .config(&"config".to_string())
            .build();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock
            .expect(vec![
                RuntimeCall::GetWorkloadId(
                    old_workload_instance_name.clone(),
                    Ok(OLD_WORKLOAD_ID.to_string()),
                ),
                RuntimeCall::DeleteWorkload(
                    OLD_WORKLOAD_ID.to_string(),
                    Err(crate::runtime_connectors::RuntimeError::Delete(
                        "some delete error".to_string(),
                    )),
                ),
                // the expectation is that create will still be called although delete failed
                RuntimeCall::CreateWorkload(
                    workload_spec.clone(),
                    Some(PIPES_LOCATION.into()),
                    to_server.clone(),
                    Ok((WORKLOAD_ID.to_string(), StubStateChecker::new())),
                ),
            ])
            .await;

        let ownable_runtime_mock: Box<dyn OwnableRuntime<String, StubStateChecker>> =
            Box::new(runtime_mock.clone());
        let test_runtime_facade = Box::new(GenericRuntimeFacade::<String, StubStateChecker>::new(
            ownable_runtime_mock,
        ));

        let _workload = test_runtime_facade.replace_workload(
            old_workload_instance_name,
            workload_spec.clone(),
            Some(control_interface_mock),
            &to_server,
        );

        tokio::task::yield_now().await;

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-delete-old-workload~1]
    #[tokio::test]
    async fn utest_runtime_facade_delete_workload() {
        let mut runtime_mock = MockRuntimeConnector::new();

        let workload_instance_name = WorkloadExecutionInstanceName::builder()
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

        test_runtime_facade.delete_workload(workload_instance_name);

        tokio::task::yield_now().await;

        runtime_mock.assert_all_expectations().await;
    }
}
