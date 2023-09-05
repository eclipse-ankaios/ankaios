use tokio::sync::mpsc;

use crate::workload_trait::Workload;
use common::objects::WorkloadExecutionInstanceName;

static COMMAND_BUFFER_SIZE: usize = 5;

#[derive(Debug)]
struct CommandChannel<W: Workload>(mpsc::Sender<WorkloadCommand<W>>);

impl<W: Workload> CommandChannel<W> {
    fn get(&self) -> &mpsc::Sender<WorkloadCommand<W>> {
        &self.0
    }
}

#[derive(Debug)]
pub struct WorkloadFacade<W>
where
    W: Workload,
{
    state: CommandChannel<W>,
    workload_name: String,
}

#[derive(Debug)]
enum WorkloadCommand<W>
where
    W: Workload,
{
    Stop,
    Update(W),
}

#[cfg(test)]
use mockall::automock;

#[cfg(test)]
lazy_static::lazy_static! {
    pub static ref MOCK_WORKLOAD_FACADE_MTX: tokio::sync::Mutex<()> = tokio::sync::Mutex::new(());
}

#[cfg_attr(test, automock)]
impl<W> WorkloadFacade<W>
where
    W: Workload + 'static + Send + Sync,
{
    // [impl->swdd~agent-facade-replace-existing-workload~1]
    pub fn replace(
        existing_instance_name: WorkloadExecutionInstanceName,
        existing_id: W::Id,
        new_workload: W,
    ) -> Self {
        let workload_name = new_workload.name();
        let (command_sender, command_receiver) = mpsc::channel(COMMAND_BUFFER_SIZE);

        tokio::spawn(async move {
            let state = match new_workload
                .replace(existing_instance_name, existing_id)
                .await
            {
                Ok(state) => Some(state),
                Err(err) => {
                    log::error!(
                        "Could not replace workload '{}'. Error: '{}'",
                        new_workload.name(),
                        err
                    );
                    None
                }
            };
            Self::await_new_command(new_workload, state, command_receiver).await
        });
        WorkloadFacade {
            state: CommandChannel(command_sender),
            workload_name,
        }
    }

    // [impl->swdd~agent-facade-resumes-existing-workload~1]
    pub fn resume(workload: W, id: W::Id) -> Self {
        let workload_name = workload.name();
        let (command_sender, command_receiver) = mpsc::channel(COMMAND_BUFFER_SIZE);

        tokio::spawn(async move {
            let state = match workload.resume(id) {
                Ok(state) => Some(state),
                Err(err) => {
                    log::error!(
                        "Could not resume workload '{}'. Error: '{}'",
                        workload.name(),
                        err
                    );
                    None
                }
            };
            Self::await_new_command(workload, state, command_receiver).await
        });
        WorkloadFacade {
            state: CommandChannel(command_sender),
            workload_name,
        }
    }

    // [impl->swdd~agent-facade-start-workload~1]
    pub fn start(workload: W) -> Self {
        let workload_name = workload.name();
        let (command_sender, command_receiver) = mpsc::channel(COMMAND_BUFFER_SIZE);

        tokio::spawn(async move {
            let state = match workload.start().await {
                Ok(state) => Some(state),
                Err(error) => {
                    log::warn!("Could not start workload '{}': {}", workload.name(), error);
                    None
                }
            };
            Self::await_new_command(workload, state, command_receiver).await;
        });

        WorkloadFacade {
            state: CommandChannel(command_sender),
            workload_name,
        }
    }

    async fn await_new_command(
        mut workload: W,
        state: Option<W::State>,
        mut command_receiver: mpsc::Receiver<WorkloadCommand<W>>,
    ) {
        let mut state = state;

        loop {
            match command_receiver.recv().await {
                // [impl->swdd~agent-facade-stops-workload~1]
                Some(WorkloadCommand::Stop) => {
                    if let Some(state) = state {
                        if let Err(error) = workload.delete(state).await {
                            log::warn!(
                                "Could not stop workload '{}': '{}'",
                                workload.name(),
                                error
                            );
                        };
                    };
                    break;
                }
                Some(WorkloadCommand::Update(new_workload)) => {
                    if let Some(current_state) = state {
                        if let Err(error) = workload.delete(current_state).await {
                            log::warn!(
                                "Could not update workload '{}': '{}'",
                                workload.name(),
                                error
                            );
                            state = None;
                            // Don't start the new workload as the previous version could not be stopped.
                            continue;
                        }
                    }

                    workload = new_workload;
                    state = match workload.start().await {
                        Ok(state) => Some(state),
                        Err(error) => {
                            log::warn!(
                                "Could not start workload '{}': '{}'",
                                workload.name(),
                                error
                            );
                            None
                        }
                    }
                }
                _ => {
                    log::warn!(
                        "Could not wait for internal stop command for workload '{}'.",
                        workload.name(),
                    );
                    return;
                }
            }
        }
    }

    // [impl->swdd~agent-facade-stops-workload~1]
    pub async fn stop(self) {
        if let Err(err) = self.state.get().send(WorkloadCommand::Stop).await {
            log::warn!(
                "Could not send internal stop command to workload '{}', error: '{}'",
                self.workload_name,
                err
            );
        }
    }

    // [impl->swdd~agent-facade-update-workload~1]
    pub async fn update(&self, workload: W) {
        if let Err(err) = self
            .state
            .get()
            .send(WorkloadCommand::Update(workload))
            .await
        {
            log::warn!(
                "Could not send internal update command to workload '{}', error: '{}'",
                self.workload_name,
                err
            );
        }
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
    use crate::{
        workload_facade::WorkloadFacade,
        workload_trait::{MockWorkload, WorkloadError},
    };
    use common::objects::WorkloadExecutionInstanceName;
    use mockall::predicate;
    use tokio::sync::oneshot;

    const RUNNING_STATE: &str = "running state";
    const NAME_1: &str = "name 1";
    const NAME_2: &str = "name 2";
    const START_ERROR: &str = "start error";
    const DELETE_ERROR: &str = "delete error";
    const CONTAINER_ID: &str = "container_id1";
    const WORKLOAD_EXECUTION_INSTANCE_NAME: &str =
        "workload.b79606fb3afea5bd1609ed40b622142f1c98125abcfe89a76a661b0e8e343910.agent";

    // [utest->swdd~agent-facade-start-workload~1]
    // [utest->swdd~agent-facade-stops-workload~1]
    #[tokio::test]
    async fn utest_workload_facade_normal_execution() {
        let (exec_start, wait_start) = oneshot::channel();
        let (exec_stop, wait_stop) = oneshot::channel();
        let (test_done, wait_test_done) = oneshot::channel();

        let mut workload = MockWorkload::new();
        workload.expect_name().return_const(NAME_1.to_string());
        workload.expect_start().once().return_once(|| {
            Box::pin(async {
                wait_start.await.unwrap();
                Ok(RUNNING_STATE.into())
            })
        });
        workload.expect_start().never(); // never is not evaluated because of an issue in mockall
        workload
            .expect_delete()
            .with(predicate::eq(RUNNING_STATE.to_string()))
            .once()
            .return_once(|_| {
                Box::pin(async {
                    wait_stop.await.unwrap();
                    test_done.send(()).unwrap();
                    Ok(())
                })
            });
        workload.expect_delete().never(); // never is not evaluated because of an issue in mockall

        let workload_facade = WorkloadFacade::start(workload);
        exec_start.send(()).unwrap();

        workload_facade.stop().await;

        exec_stop.send(()).unwrap();
        wait_test_done.await.unwrap();
    }

    // [utest->swdd~agent-facade-start-workload~1]
    #[tokio::test]
    async fn utest_workload_facade_start_error() {
        let (exec_start, wait_start) = oneshot::channel();

        let mut workload = MockWorkload::new();
        workload.expect_name().return_const(NAME_1.to_string());
        workload.expect_start().once().return_once(|| {
            Box::pin(async {
                wait_start.await.unwrap();
                Err(WorkloadError::StartError(START_ERROR.to_string()))
            })
        });
        workload.expect_start().never(); // never is not evaluated because of an issue in mockall
        workload.expect_delete().never(); // never is not evaluated because of an issue in mockall

        let workload_facade = WorkloadFacade::start(workload);
        exec_start.send(()).unwrap();

        workload_facade.stop().await;
    }

    // [utest->swdd~agent-facade-resumes-existing-workload~1]
    #[tokio::test]
    async fn utest_workload_facade_resume() {
        let (exec_stop, wait_stop) = oneshot::channel();
        let (test_done, wait_test_done) = oneshot::channel();

        let mut workload = MockWorkload::new();
        workload.expect_name().return_const(NAME_1.to_string());
        workload
            .expect_resume()
            .with(predicate::eq(CONTAINER_ID.to_string()))
            .once()
            .return_once(|_| Ok(RUNNING_STATE.to_string()));

        workload.expect_resume().never(); // never is not evaluated because of an issue in mockall

        workload
            .expect_delete()
            .with(predicate::eq(RUNNING_STATE.to_string()))
            .once()
            .return_once(|_| {
                Box::pin(async {
                    wait_stop.await.unwrap();
                    test_done.send(()).unwrap();
                    Ok(())
                })
            });
        workload.expect_delete().never(); // never is not evaluated because of an issue in mockall

        let workload_facade = WorkloadFacade::resume(workload, CONTAINER_ID.to_string());

        workload_facade.stop().await;
        exec_stop.send(()).unwrap();
        wait_test_done.await.unwrap();
    }

    // [utest->swdd~agent-facade-replace-existing-workload~1]
    #[tokio::test]
    async fn utest_workload_facade_replace() {
        let (exec_replace, wait_replace) = oneshot::channel();
        let (exec_stop, wait_stop) = oneshot::channel();
        let (test_done, wait_test_done) = oneshot::channel();

        let existing_instance_name_workload =
            WorkloadExecutionInstanceName::new(WORKLOAD_EXECUTION_INSTANCE_NAME).unwrap();

        let existing_instance_name_facade = existing_instance_name_workload.clone();

        let mut new_workload = MockWorkload::new();
        new_workload.expect_name().return_const(NAME_1.to_string());
        new_workload
            .expect_replace()
            .with(
                predicate::eq(existing_instance_name_workload),
                predicate::eq(CONTAINER_ID.to_string()),
            )
            .once()
            .return_once(|_, _| {
                Box::pin(async {
                    wait_replace.await.unwrap();
                    Ok(RUNNING_STATE.into())
                })
            });

        new_workload.expect_replace().never(); // never is not evaluated because of an issue in mockall

        new_workload
            .expect_delete()
            .with(predicate::eq(RUNNING_STATE.to_string()))
            .once()
            .return_once(|_| {
                Box::pin(async {
                    wait_stop.await.unwrap();
                    test_done.send(()).unwrap();
                    Ok(())
                })
            });
        new_workload.expect_delete().never(); // never is not evaluated because of an issue in mockall

        let workload_facade = WorkloadFacade::replace(
            existing_instance_name_facade,
            CONTAINER_ID.into(),
            new_workload,
        );
        exec_replace.send(()).unwrap();

        workload_facade.stop().await;
        exec_stop.send(()).unwrap();
        wait_test_done.await.unwrap();
    }

    // [utest->swdd~agent-facade-replace-existing-workload~1]
    #[tokio::test]
    async fn utest_workload_facade_replace_error() {
        let (exec_replace, wait_replace) = oneshot::channel();

        let mut workload = MockWorkload::new();
        workload.expect_name().return_const(NAME_1.to_string());
        workload.expect_replace().once().return_once(|_, _| {
            Box::pin(async {
                wait_replace.await.unwrap();
                Err(WorkloadError::StartError(START_ERROR.to_string()))
            })
        });
        workload.expect_start().never(); // never is not evaluated because of an issue in mockall
        workload.expect_delete().never(); // never is not evaluated because of an issue in mockall

        let existing_instance_name =
            WorkloadExecutionInstanceName::new(WORKLOAD_EXECUTION_INSTANCE_NAME).unwrap();
        let workload_facade =
            WorkloadFacade::replace(existing_instance_name, CONTAINER_ID.to_string(), workload);
        exec_replace.send(()).unwrap();

        workload_facade.stop().await;
    }

    // [utest->swdd~agent-facade-update-workload~1]
    #[tokio::test]
    async fn utest_workload_facade_update() {
        let (exec_start, wait_start) = oneshot::channel();
        let (exec_delete, wait_delete) = oneshot::channel();
        let (exec_update, wait_update) = oneshot::channel();
        let (exec_stop, wait_stop) = oneshot::channel();
        let (test_done, wait_test_done) = oneshot::channel();

        let mut old_workload = MockWorkload::new();
        old_workload.expect_name().return_const(NAME_1.to_string());
        old_workload.expect_start().once().return_once(|| {
            Box::pin(async {
                wait_start.await.unwrap();
                Ok(RUNNING_STATE.to_string())
            })
        });

        old_workload.expect_delete().once().return_once(|_| {
            Box::pin(async {
                wait_delete.await.unwrap();
                Ok(())
            })
        });

        let mut new_workload = MockWorkload::new();
        new_workload.expect_name().return_const(NAME_2.to_string());
        new_workload.expect_start().once().return_once(|| {
            Box::pin(async {
                wait_update.await.unwrap();
                Ok(RUNNING_STATE.into())
            })
        });
        new_workload
            .expect_delete()
            .with(predicate::eq(RUNNING_STATE.to_string()))
            .once()
            .return_once(|_| {
                Box::pin(async {
                    wait_stop.await.unwrap();
                    test_done.send(()).unwrap();
                    Ok(())
                })
            });
        new_workload.expect_delete().never(); // never is not evaluated because of an issue in mockall

        let workload_facade = WorkloadFacade::start(old_workload); // start with a current workload
        exec_start.send(()).unwrap();

        workload_facade.update(new_workload).await; // update old workload with new workload
        exec_update.send(()).unwrap();
        exec_delete.send(()).unwrap();

        workload_facade.stop().await; // delete the new workload
        exec_stop.send(()).unwrap();
        wait_test_done.await.unwrap();
    }

    // [utest->swdd~agent-facade-update-workload~1]
    #[tokio::test]
    async fn utest_workload_facade_update_error_on_new_workload_start() {
        let (exec_start, wait_start) = oneshot::channel();
        let (exec_delete, wait_delete) = oneshot::channel();
        let (exec_update, wait_update) = oneshot::channel();

        let mut old_workload = MockWorkload::new();
        old_workload.expect_name().return_const(NAME_1.to_string());
        old_workload.expect_start().once().return_once(|| {
            Box::pin(async {
                wait_start.await.unwrap();
                Ok(RUNNING_STATE.to_string())
            })
        });

        old_workload.expect_delete().once().return_once(|_| {
            Box::pin(async {
                wait_delete.await.unwrap();
                Ok(())
            })
        });

        let mut new_workload = MockWorkload::new();
        new_workload.expect_name().return_const(NAME_2.to_string());
        new_workload.expect_start().once().return_once(|| {
            Box::pin(async {
                wait_update.await.unwrap();
                Err(WorkloadError::StartError(START_ERROR.to_string()))
            })
        });
        new_workload.expect_delete().never(); // never is not evaluated because of an issue in mockall

        let workload_facade = WorkloadFacade::start(old_workload); // start with a current workload
        exec_start.send(()).unwrap();

        workload_facade.update(new_workload).await; // update old workload with new workload fails when new workload is started
        exec_update.send(()).unwrap();
        exec_delete.send(()).unwrap();

        workload_facade.stop().await; // when delete is called the new workload is not deleted because it was not started
    }

    // [utest->swdd~agent-facade-update-workload~1]
    #[tokio::test]
    async fn utest_workload_facade_update_error_on_old_workload_delete() {
        let (exec_start, wait_start) = oneshot::channel();
        let (exec_delete, wait_delete) = oneshot::channel();
        let (test_done, wait_test_done) = oneshot::channel();

        let mut old_workload = MockWorkload::new();
        old_workload
            .expect_name()
            .once()
            .return_const(NAME_1.to_string());
        old_workload.expect_start().once().return_once(|| {
            Box::pin(async {
                wait_start.await.unwrap();
                Ok(RUNNING_STATE.to_string())
            })
        });

        old_workload.expect_delete().once().return_once(|_| {
            Box::pin(async {
                wait_delete.await.unwrap();
                test_done.send(()).unwrap();
                Err(WorkloadError::DeleteError(DELETE_ERROR.to_string()))
            })
        });

        let mut new_workload = MockWorkload::new();
        new_workload
            .expect_name()
            .once()
            .return_const(NAME_2.to_string());

        new_workload.expect_start().never(); // never is not evaluated because of an issue in mockall
        new_workload.expect_delete().never(); // never is not evaluated because of an issue in mockall

        let workload_facade = WorkloadFacade::start(old_workload); // start with a current workload
        exec_start.send(()).unwrap();

        workload_facade.update(new_workload).await; // update old workload with new workload fails when old workload is deleted
        exec_delete.send(()).unwrap();

        workload_facade.stop().await; // when delete is called the new workload is not deleted because it was not started
        wait_test_done.await.unwrap();
    }
}
