use async_trait::async_trait;

use crate::runtime_connectors::log_collector::LogCollector;

#[derive(Debug)]
pub struct PodmanLogCollector {}

#[async_trait]
impl LogCollector for PodmanLogCollector {
    async fn next_lines(&mut self) -> Option<Vec<String>> {
        None
    }
}
