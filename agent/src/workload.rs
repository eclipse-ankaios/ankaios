use std::path::PathBuf;

use crate::{
    control_interface::PipesChannelContext,
    runtime::{Runtime, RuntimeError},
    state_checker::StateChecker,
};
use common::{
    commands::CompleteState, execution_interface::ExecutionCommand, objects::WorkloadSpec,
    state_change_interface::StateChangeSender,
};
use tokio::sync::mpsc;

#[derive(Debug)]
pub enum WorkloadCommand {
    Stop,
    Update(Box<WorkloadSpec>, Option<PathBuf>),
}

// #[derive(Debug)]
pub struct Workload {
    channel: mpsc::Sender<WorkloadCommand>,
    control_interface: Option<PipesChannelContext>,
}

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

    pub async fn update(
        &mut self,
        spec: WorkloadSpec,
        control_interface: Option<PipesChannelContext>,
    ) -> Result<(), RuntimeError> {
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
            .map_err(|err| RuntimeError::Update(err.to_string()))
    }

    pub async fn delete(self) -> Result<(), RuntimeError> {
        if let Some(control_interface) = self.control_interface {
            control_interface.abort_pipes_channel_task()
        }

        self.channel
            .send(WorkloadCommand::Stop)
            .await
            .map_err(|err| RuntimeError::Delete(err.to_string()))
    }

    pub async fn await_new_command<WorkloadId, StChecker>(
        workload_name: String,
        initial_workload_id: Option<WorkloadId>,
        initial_state_checker: Option<StChecker>,
        update_state_tx: StateChangeSender,
        runtime: Box<dyn Runtime<WorkloadId, StChecker>>,
        mut command_receiver: mpsc::Receiver<WorkloadCommand>,
    ) where
        WorkloadId: Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
    {
        let mut state_checker = initial_state_checker;
        let mut workload_id = initial_workload_id;
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
                Some(WorkloadCommand::Update(runtime_workload_config, control_interface_path)) => {
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

    pub async fn send_complete_state(
        &mut self,
        complete_state: CompleteState,
    ) -> Result<(), RuntimeError> {
        let control_interface =
            self.control_interface
                .as_ref()
                .ok_or(RuntimeError::CompleteState(
                    "control interface not available".to_string(),
                ))?;
        control_interface
            .get_input_pipe_sender()
            .send(ExecutionCommand::CompleteState(Box::new(complete_state)))
            .await
            .map_err(|err| RuntimeError::CompleteState(err.to_string()))
    }
}
