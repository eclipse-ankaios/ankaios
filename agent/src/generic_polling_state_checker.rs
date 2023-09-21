use crate::stoppable_state_checker::StoppableStateChecker;
use async_trait::async_trait;

#[derive(Debug)]
pub struct GenericPollingStateChecker {}

#[async_trait]
impl StoppableStateChecker for GenericPollingStateChecker {
    async fn stop_checker(self) {
        todo!()
    }
}
