use async_trait::async_trait;
use common::objects::{RuntimeWorkload, WorkloadExecutionInstanceName};
use tokio::sync::mpsc;

use crate::{
    runtime::{OwnableRuntime, Runtime},
    stoppable_state_checker::StoppableStateChecker,
    workload::{Workload, WorkloadCommand},
};

static COMMAND_BUFFER_SIZE: usize = 5;

#[async_trait]
pub trait WorkloadFactory {
    fn create_workload(&self, runtime_workload: RuntimeWorkload) -> Workload;

    fn replace_workload(
        &self,
        new_workload_config: RuntimeWorkload,
        existing_Workload_name: WorkloadExecutionInstanceName,
    ) -> Workload;

    fn resume_workload(
        &self,
        runtime_workload: RuntimeWorkload,
        existing_id: WorkloadExecutionInstanceName,
    ) -> Workload;
}

pub struct GenericWorkloadFactory<
    WorkloadId: Send + Sync,
    StateChecker: StoppableStateChecker + Send + Sync,
> {
    runtime: dyn OwnableRuntime<WorkloadId, StateChecker>,
}

impl<WorkloadId, StateChecker> GenericWorkloadFactory<WorkloadId, StateChecker>
where
    WorkloadId: Send + Sync + 'static,
    StateChecker: StoppableStateChecker + Send + Sync + 'static,
{
    async fn await_new_command(
        workload_name: String,
        initial_workload_id: WorkloadId,
        initial_state_checker: StateChecker,
        runtime: Box<dyn Runtime<WorkloadId, StateChecker>>,
        mut command_receiver: mpsc::Receiver<WorkloadCommand>,
    ) {
        let mut state_checker = Some(initial_state_checker);
        let mut workload_id = Some(initial_workload_id);
        loop {
            match command_receiver.recv().await {
                // [impl->swdd~agent-facade-stops-workload~1]
                Some(WorkloadCommand::Stop) => {
                    if let Some(old_id) = workload_id.take() {
                        if let Err(err) = runtime.delete_workload(&old_id).await {
                            log::warn!("Could not stop workload '{}': '{}'", workload_name, err);
                        } else {
                            log::debug!("Stop workload complete");
                        }
                    } else {
                        log::debug!("Workload '{}' already gone.", workload_name);
                    }
                    return;
                }
                Some(WorkloadCommand::Update(runtime_workload_config)) => {
                    if let Some(old_id) = workload_id {
                        if let Err(err) = runtime.delete_workload(&old_id).await {
                            log::warn!("Could not update workload '{}': '{}'", workload_name, err);
                            workload_id = Some(old_id);
                            continue;
                        } else {
                            workload_id = None;
                            if let Some(old_checker) = state_checker.take() {
                                old_checker.stop_checker().await;
                            }
                        }
                    } else {
                        log::debug!("Workload '{}' already gone.", workload_name);
                    }

                    match runtime.create_workload(runtime_workload_config).await {
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
}

#[async_trait]
impl<
        WorkloadId: Send + Sync + 'static,
        StateChecker: StoppableStateChecker + Send + Sync + 'static,
    > WorkloadFactory for GenericWorkloadFactory<WorkloadId, StateChecker>
{
    // [impl->swdd~agent-facade-start-workload~1]
    fn create_workload(&self, new_workload_config: RuntimeWorkload) -> Workload {
        let (command_sender, command_receiver) = mpsc::channel(COMMAND_BUFFER_SIZE);

        let workload_name = new_workload_config.name.clone();
        let runtime = self.runtime.to_owned();

        let task_handle = tokio::spawn(async move {
            let (workload_id, state_checker) =
                runtime.create_workload(new_workload_config).await.unwrap();

            Self::await_new_command(
                workload_name,
                workload_id,
                state_checker,
                runtime,
                command_receiver,
            )
            .await;
        });

        Workload {
            channel: command_sender,
            task_handle,
        }
    }

    // [impl->swdd~agent-facade-replace-existing-workload~1]
    fn replace_workload(
        &self,
        new_workload_config: RuntimeWorkload,
        existing_id: WorkloadExecutionInstanceName,
    ) -> Workload {
        let (command_sender, command_receiver) = mpsc::channel(COMMAND_BUFFER_SIZE);

        let workload_name = new_workload_config.name.clone();
        let runtime = self.runtime.to_owned();

        let task_handle = tokio::spawn(async move {
            let old_id = runtime.get_workload_id(existing_id).await.unwrap();

            runtime.delete_workload(&old_id).await.unwrap();

            let (workload_id, state_checker) =
                runtime.create_workload(new_workload_config).await.unwrap();

            Self::await_new_command(
                workload_name,
                workload_id,
                state_checker,
                runtime,
                command_receiver,
            )
            .await;
        });

        Workload {
            channel: command_sender,
            task_handle,
        }
    }

    // [impl->swdd~agent-facade-resumes-existing-workload~1]
    fn resume_workload(
        &self,
        new_workload_config: RuntimeWorkload,
        existing_id: WorkloadExecutionInstanceName,
    ) -> Workload {
        let (command_sender, command_receiver) = mpsc::channel(COMMAND_BUFFER_SIZE);

        let workload_name = new_workload_config.name.clone();
        let runtime = self.runtime.to_owned();

        let task_handle = tokio::spawn(async move {
            let workload_id = runtime.get_workload_id(existing_id).await.unwrap();

            let state_checker = runtime
                .start_checker(&workload_id, new_workload_config)
                .await
                .unwrap();

            Self::await_new_command(
                workload_name,
                workload_id,
                state_checker,
                runtime,
                command_receiver,
            )
            .await;
        });

        Workload {
            channel: command_sender,
            task_handle,
        }
    }
}
