use async_trait::async_trait;
use common::{
    objects::{AgentName, WorkloadExecutionInstanceName, WorkloadInstanceName, WorkloadSpec},
    state_change_interface::StateChangeSender,
    std_extensions::IllegalStateResult,
};
use tokio::sync::mpsc;

use crate::{
    control_interface::PipesChannelContext,
    runtime::{OwnableRuntime, Runtime, RuntimeError},
    state_checker::StateChecker,
    workload::{Workload, WorkloadCommand},
};

static COMMAND_BUFFER_SIZE: usize = 5;

#[async_trait]
pub trait RuntimeFacade: Send + Sync {
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
    WorkloadId: Send + Sync,
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
    async fn get_reusable_running_workloads(
        &self,
        agent_name: &AgentName,
    ) -> Result<Vec<WorkloadExecutionInstanceName>, RuntimeError> {
        self.runtime
            .get_reusable_running_workloads(agent_name)
            .await
    }

    // [impl->swdd~agent-facade-start-workload~1]
    fn create_workload(
        &self,
        workload_spec: WorkloadSpec,
        control_interface: Option<PipesChannelContext>,
        update_state_tx: &StateChangeSender,
    ) -> Workload {
        let (command_sender, command_receiver) = mpsc::channel(COMMAND_BUFFER_SIZE);

        let workload_name = workload_spec.name.clone();
        let runtime = self.runtime.to_owned();
        let update_state_tx = update_state_tx.clone();
        let control_interface_path = control_interface
            .as_ref()
            .map(|control_interface| control_interface.get_api_location());

        tokio::spawn(async move {
            let (workload_id, state_checker) = runtime
                .create_workload(
                    workload_spec,
                    control_interface_path,
                    update_state_tx.clone(),
                )
                .await
                .map_or_else(
                    |err| {
                        log::warn!("Failed to create workload: '{}': '{}'", workload_name, err);
                        (None, None)
                    },
                    |(workload_id, state_checker)| (Some(workload_id), Some(state_checker)),
                );

            Workload::await_new_command(
                workload_name,
                workload_id,
                state_checker,
                update_state_tx,
                runtime,
                command_receiver,
            )
            .await;
        });

        Workload::new(command_sender, control_interface)
    }

    // [impl->swdd~agent-facade-replace-existing-workload~1]
    fn replace_workload(
        &self,
        old_instance_name: WorkloadExecutionInstanceName,
        new_workload_spec: WorkloadSpec,
        control_interface: Option<PipesChannelContext>,
        update_state_tx: &StateChangeSender,
    ) -> Workload {
        let (command_sender, command_receiver) = mpsc::channel(COMMAND_BUFFER_SIZE);

        let workload_name = new_workload_spec.name.clone();
        let runtime = self.runtime.to_owned();
        let update_state_tx = update_state_tx.clone();
        let control_interface_path = control_interface
            .as_ref()
            .map(|control_interface| control_interface.get_api_location());

        tokio::spawn(async move {
            let old_workload_name = old_instance_name.workload_name();
            match runtime.get_workload_id(&old_instance_name).await {
                Ok(old_id) => runtime
                    .delete_workload(&old_id)
                    .await
                    .unwrap_or_else(|err| {
                        log::warn!(
                            "Failed to delete workload when replacing workload '{}': '{}'",
                            old_workload_name,
                            err
                        )
                    }),
                Err(err) => log::warn!(
                    "Failed to get workload id when replacing workload '{}': '{}'",
                    old_workload_name,
                    err
                ),
            }

            let (workload_id, state_checker) = runtime
                .create_workload(
                    new_workload_spec,
                    control_interface_path,
                    update_state_tx.clone(),
                )
                .await
                .map_or_else(
                    |err| {
                        log::warn!(
                            "Failed to create workload when replacing workload '{}': '{}'",
                            old_workload_name,
                            err
                        );
                        (None, None)
                    },
                    |(workload_id, state_checker)| (Some(workload_id), Some(state_checker)),
                );

            // replace workload_id and state_checker through Option directly and pass in None if create_workload fails
            Workload::await_new_command(
                workload_name,
                workload_id,
                state_checker,
                update_state_tx,
                runtime,
                command_receiver,
            )
            .await;
        });

        Workload::new(command_sender, control_interface)
    }

    // [impl->swdd~agent-facade-resumes-existing-workload~1]
    fn resume_workload(
        &self,
        workload_spec: WorkloadSpec,
        control_interface: Option<PipesChannelContext>,
        update_state_tx: &StateChangeSender,
    ) -> Workload {
        let (command_sender, command_receiver) = mpsc::channel(COMMAND_BUFFER_SIZE);

        let workload_name = workload_spec.name.clone();
        let runtime = self.runtime.to_owned();
        let update_state_tx = update_state_tx.clone();

        tokio::spawn(async move {
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

            Workload::await_new_command(
                workload_name,
                workload_id.ok(),
                state_checker,
                update_state_tx,
                runtime,
                command_receiver,
            )
            .await;
        });

        Workload::new(command_sender, control_interface)
    }

    fn delete_workload(&self, instance_name: WorkloadExecutionInstanceName) {
        let runtime = self.runtime.to_owned();
        tokio::spawn(async move {
            runtime
                .delete_workload(&runtime.get_workload_id(&instance_name).await?)
                .await
        });
    }
}
