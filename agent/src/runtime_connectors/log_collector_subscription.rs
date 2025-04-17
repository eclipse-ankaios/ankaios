use std::ops::DerefMut;

use tokio::task::JoinHandle;

use super::{
    log_channel,
    log_collector::{self, LogCollector},
};

pub struct LogCollectorSubscription {
    join_handles: Vec<JoinHandle<()>>,
}

impl LogCollectorSubscription {
    pub fn start_collecting_logs(
        log_collectors: Vec<impl DerefMut<Target: LogCollector> + Send + 'static>,
    ) -> (Self, Vec<log_channel::Receiver>) {
        let (join_handles, receivers) = log_collectors
            .into_iter()
            .map(|x| {
                let (sender, receiver) = log_channel::channel();
                let jh = tokio::spawn(async move {
                    log_collector::run(x, sender).await;
                });
                (jh, receiver)
            })
            .unzip();

        (Self { join_handles }, receivers)
    }
}

impl Drop for LogCollectorSubscription {
    fn drop(&mut self) {
        self.join_handles.iter().for_each(|x| x.abort());
    }
}
