use async_trait::async_trait;

#[async_trait]
pub trait StoppableStateChecker {
    async fn stop_checker(self);
}
