#[cfg(test)]
use mockall::mock;
use rand::Rng;
use std::time::Duration;
use std::{cmp::min, future::Future};

use tokio::select;
use tokio::sync::watch;

#[cfg(not(test))]
use rand::rng;
#[cfg(test)]
use tests::rng;

#[cfg(test)]
use tests::sleep;
#[cfg(not(test))]
use tokio::time::sleep;

const BASE_BACKOFF_IN_MILLIS: u64 = 500;
const BACKOFF_MAX_IN_MILLIS: u64 = 300000;

#[derive(Default)]
pub struct RetryManager {
    current_is_valid: watch::Sender<bool>,
}

#[derive(Debug)]
pub struct RetryToken {
    counter: u32,
    valid: watch::Receiver<bool>,
}

impl RetryManager {
    // [impl->swdd~agent-workload-control-loop-prevents-retries-on-other-workload-commands~2]
    pub fn invalidate(&mut self) {
        _ = self.current_is_valid.send(false);
    }

    pub fn new_token(&mut self) -> RetryToken {
        // [impl->swdd~agent-workload-control-loop-prevents-retries-on-other-workload-commands~2]
        self.invalidate();

        let (sender, receiver) = watch::channel(true);
        self.current_is_valid = sender;

        // [impl->swdd~agent-workload-control-loop-reset-backoff-on-update]
        RetryToken {
            counter: 0,
            valid: receiver,
        }
    }
}

impl Drop for RetryManager {
    fn drop(&mut self) {
        self.invalidate();
    }
}

impl RetryToken {
    pub fn counter(&self) -> u32 {
        self.counter
    }

    fn inc_counter(&mut self) {
        self.counter += 1;
    }

    // [impl->swdd~agent-workload-control-loop-prevents-retries-on-other-workload-commands~2]
    pub fn is_valid(&self) -> bool {
        *self.valid.borrow()
    }
    // [impl->swdd~agent-workload-control-loop-exponential-backoff-retries~1]
    pub async fn call_with_backoff<F, R>(mut self, f: F)
    where
        F: FnOnce(Self) -> R,
        R: Future,
    {
        let timeout = self.calc_backoff_with_jitter();
        self.inc_counter();

        select! {
            _val = self.valid.wait_for(|x| !x) => {
                log::debug!("Timeout for retry interrupted");
                return;
            }
            _val = sleep(timeout) => {
            }
        };
        f(self).await;
    }

    fn calc_backoff_with_jitter(&mut self) -> Duration {
        let maximal_backoff = self.calc_backoff();
        let time_in_millis = rng().random_range(..=maximal_backoff);
        Duration::from_millis(time_in_millis)
    }

    fn calc_backoff(&self) -> u64 {
        min(
            2u64.pow(self.counter) * BASE_BACKOFF_IN_MILLIS,
            BACKOFF_MAX_IN_MILLIS,
        )
    }
}

#[cfg(test)]
#[derive(Debug, PartialEq)]
pub struct MockRetryToken {
    pub mock_id: u32,
    pub valid: bool,
    pub has_been_called: bool,
}

#[cfg(test)]
impl MockRetryToken {
    pub async fn call_with_backoff<F, R>(mut self, f: F)
    where
        F: FnOnce(Self) -> R,
        R: Future,
    {
        self.has_been_called = true;
        f(self).await;
    }

    pub fn counter(&self) -> u32 {
        0
    }

    pub fn is_valid(&self) -> bool {
        self.valid
    }
}

#[cfg(test)]
mock! {
    pub RetryManager {
        pub fn invalidate(&mut self);
        pub fn new_token(&mut self) -> MockRetryToken;
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
    use std::{
        cell::RefCell,
        ops::{DerefMut, RangeToInclusive},
        pin::{pin, Pin},
        rc::Rc,
        sync::{Arc, Mutex},
        task::{Context, Poll, Wake, Waker},
        time::Duration,
    };

    use futures_util::Future;
    use tokio::sync::oneshot;

    use super::RetryToken;

    static TEST_MUTEX: Mutex<()> = Mutex::new(());

    static SLEEP_SENDER: Mutex<Option<oneshot::Sender<()>>> = Mutex::new(None);
    static SLEEP_LAST_DURATION: Mutex<Option<Duration>> = Mutex::new(None);

    static INITIAL_RANDOM: u64 = 1;
    static LAST_RANDOM_VALUE: Mutex<u64> = Mutex::new(INITIAL_RANDOM);
    static RANDOM_OFFSET_1: u64 = INITIAL_RANDOM;
    static RANDOM_OFFSET_2: u64 = next_random(RANDOM_OFFSET_1);
    static RANDOM_OFFSET_3: u64 = next_random(RANDOM_OFFSET_2);
    static RANDOM_OFFSET_4: u64 = next_random(RANDOM_OFFSET_3);
    static RANDOM_OFFSET_5: u64 = next_random(RANDOM_OFFSET_4);

    pub fn sleep(duration: Duration) -> oneshot::Receiver<()> {
        let (sender, receiver) = oneshot::channel();
        *SLEEP_SENDER.lock().unwrap() = Some(sender);
        *SLEEP_LAST_DURATION.lock().unwrap() = Some(duration);
        receiver
    }

    pub struct MockRng {}

    impl MockRng {
        pub fn random_range(&self, range: RangeToInclusive<u64>) -> u64 {
            let mut lock = LAST_RANDOM_VALUE.lock().unwrap();
            let current_random_value = *lock;
            *lock = next_random(current_random_value);
            range.end - current_random_value
        }
    }

    const fn next_random(current: u64) -> u64 {
        (current + 3) % 5
    }

    pub fn rng() -> MockRng {
        MockRng {}
    }

    fn reset_random() {
        let mut lock = LAST_RANDOM_VALUE.lock().unwrap();
        *lock = INITIAL_RANDOM;
    }

    struct SimpleWake();

    impl Wake for SimpleWake {
        fn wake(self: std::sync::Arc<Self>) {}
    }

    #[derive(Default)]
    struct SimpleFuture {
        has_been_called: bool,
    }

    impl Future for SimpleFuture {
        type Output = ();

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            if self.has_been_called {
                Poll::Ready(())
            } else {
                self.deref_mut().has_been_called = true;
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }

    fn create_callback() -> (
        impl FnOnce(RetryToken) -> Pin<Box<dyn Future<Output = ()>>>,
        Rc<RefCell<Option<RetryToken>>>,
    ) {
        let called = Rc::new(RefCell::new(None));
        let called_copy = called.clone();
        fn callback(
            called: Rc<RefCell<Option<RetryToken>>>,
            retry_token: RetryToken,
        ) -> Pin<Box<dyn Future<Output = ()>>> {
            *called.borrow_mut() = Some(retry_token);
            Box::pin(SimpleFuture::default())
        }
        (
            move |retry_token: RetryToken| Box::pin(callback(called_copy, retry_token)),
            called,
        )
    }

    fn waker() -> Rc<Waker> {
        Rc::new(Waker::from(Arc::new(SimpleWake())))
    }

    fn assert_sleep_for_millis(millis: u64) {
        assert_eq!(
            *SLEEP_LAST_DURATION.lock().unwrap(),
            Some(Duration::from_millis(millis))
        );
        SLEEP_SENDER
            .lock()
            .unwrap()
            .take()
            .unwrap()
            .send(())
            .unwrap();
    }

    // [utest->swdd~agent-workload-control-loop-exponential-backoff-retries~1]
    #[test]
    fn utest_one_retry() {
        let _lock = TEST_MUTEX.lock().unwrap();
        reset_random();
        let mut manager = super::RetryManager::default();
        let token = manager.new_token();

        let (callback, called) = create_callback();

        let waker = waker();
        let mut context = Context::from_waker(&waker);

        assert_eq!(token.counter(), 0);
        let mut call_res = pin!(token.call_with_backoff(callback));
        assert_eq!(call_res.as_mut().poll(&mut context), Poll::Pending);
        assert!(called.borrow().is_none());
        assert_sleep_for_millis(500 - RANDOM_OFFSET_1);

        assert_eq!(call_res.as_mut().poll(&mut context), Poll::Pending);
        assert_eq!(call_res.as_mut().poll(&mut context), Poll::Ready(()));
        assert!(called.borrow().is_some());
        let token = called.borrow_mut().take().unwrap();
        assert_eq!(token.counter(), 1);
    }

    // [utest->swdd~agent-workload-control-loop-exponential-backoff-retries~1]
    #[test]
    fn utest_max_retry_time() {
        trait TimeoutAssertion {
            fn assert_timeout_in_millis(self: Box<Self>, millis: u64) -> Box<Self>;
        }

        impl TimeoutAssertion for RetryToken {
            fn assert_timeout_in_millis(self: Box<Self>, millis: u64) -> Box<Self> {
                let waker = waker();
                let mut context = Context::from_waker(&waker);

                let (callback, called) = create_callback();
                let mut call_res = pin!(self.call_with_backoff(callback));
                assert_eq!(call_res.as_mut().poll(&mut context), Poll::Pending);
                assert!(called.borrow().is_none());
                assert_sleep_for_millis(millis);

                assert_eq!(call_res.as_mut().poll(&mut context), Poll::Pending);
                assert_eq!(call_res.as_mut().poll(&mut context), Poll::Ready(()));
                assert!(called.borrow().is_some());
                let token = called.borrow_mut().take().unwrap();
                Box::new(token)
            }
        }

        let _lock = TEST_MUTEX.lock().unwrap();
        reset_random();
        let mut manager = super::RetryManager::default();
        let token = Box::new(manager.new_token());

        assert_eq!(token.counter(), 0);
        let token = token
            .assert_timeout_in_millis(500 - RANDOM_OFFSET_1)
            .assert_timeout_in_millis(1000 - RANDOM_OFFSET_2)
            .assert_timeout_in_millis(2000 - RANDOM_OFFSET_3)
            .assert_timeout_in_millis(4000 - RANDOM_OFFSET_4)
            .assert_timeout_in_millis(8000 - RANDOM_OFFSET_5)
            .assert_timeout_in_millis(16000 - RANDOM_OFFSET_1)
            .assert_timeout_in_millis(32000 - RANDOM_OFFSET_2)
            .assert_timeout_in_millis(64000 - RANDOM_OFFSET_3)
            .assert_timeout_in_millis(128000 - RANDOM_OFFSET_4)
            .assert_timeout_in_millis(256000 - RANDOM_OFFSET_5)
            .assert_timeout_in_millis(300000 - RANDOM_OFFSET_1);
        assert_eq!(token.counter(), 11);
    }

    // [utest->swdd~agent-workload-control-loop-reset-backoff-on-update]
    #[test]
    fn utest_new_toke_resets_timeout() {
        let _lock = TEST_MUTEX.lock().unwrap();
        reset_random();
        let mut manager = super::RetryManager::default();

        let waker = waker();
        let mut context = Context::from_waker(&waker);

        let token = manager.new_token();

        assert_eq!(token.counter(), 0);
        let (callback, called) = create_callback();
        let mut call_res = pin!(token.call_with_backoff(callback));
        assert_eq!(call_res.as_mut().poll(&mut context), Poll::Pending);
        assert!(called.borrow().is_none());
        assert_sleep_for_millis(500 - RANDOM_OFFSET_1);

        assert_eq!(call_res.as_mut().poll(&mut context), Poll::Pending);
        assert_eq!(call_res.as_mut().poll(&mut context), Poll::Ready(()));
        assert!(called.borrow().is_some());
        let token = called.borrow_mut().take().unwrap();
        assert_eq!(token.counter(), 1);

        let (callback, called) = create_callback();
        let mut call_res = pin!(token.call_with_backoff(callback));
        assert_eq!(call_res.as_mut().poll(&mut context), Poll::Pending);
        assert!(called.borrow().is_none());
        assert_sleep_for_millis(1000 - RANDOM_OFFSET_2);

        assert_eq!(call_res.as_mut().poll(&mut context), Poll::Pending);
        assert_eq!(call_res.as_mut().poll(&mut context), Poll::Ready(()));
        assert!(called.borrow().is_some());
        let token = called.borrow_mut().take().unwrap();
        assert_eq!(token.counter(), 2);

        let token = manager.new_token();

        assert_eq!(token.counter(), 0);
        let (callback, called) = create_callback();
        let mut call_res = pin!(token.call_with_backoff(callback));
        assert_eq!(call_res.as_mut().poll(&mut context), Poll::Pending);
        assert!(called.borrow().is_none());
        assert_sleep_for_millis(500 - RANDOM_OFFSET_3);

        assert_eq!(call_res.as_mut().poll(&mut context), Poll::Pending);
        assert_eq!(call_res.as_mut().poll(&mut context), Poll::Ready(()));
        assert!(called.borrow().is_some());
        let token = called.borrow_mut().take().unwrap();
        assert_eq!(token.counter(), 1);
    }

    // [utest->swdd~agent-workload-control-loop-prevents-retries-on-other-workload-commands~2]
    #[test]
    fn utest_invalidate_invalidates_token() {
        let _lock = TEST_MUTEX.lock().unwrap();
        reset_random();
        let mut manager = super::RetryManager::default();
        let token = manager.new_token();
        assert!(token.is_valid());
        manager.invalidate();
        assert!(!token.is_valid());
    }

    // [utest->swdd~agent-workload-control-loop-prevents-retries-on-other-workload-commands~2]
    #[test]
    fn utest_invalidate_stops_retry() {
        let _lock = TEST_MUTEX.lock().unwrap();
        reset_random();
        let mut manager = super::RetryManager::default();
        let token = manager.new_token();

        let (callback, called) = create_callback();

        let waker = waker();
        let mut context = Context::from_waker(&waker);

        let mut call_res = pin!(token.call_with_backoff(callback));
        assert_eq!(call_res.as_mut().poll(&mut context), Poll::Pending);
        assert!(called.borrow().is_none());

        manager.invalidate();

        assert_eq!(call_res.as_mut().poll(&mut context), Poll::Ready(()));
        assert!(called.borrow().is_none());
    }

    // [utest->swdd~agent-workload-control-loop-prevents-retries-on-other-workload-commands~2]
    #[test]
    fn utest_new_token_invalidates_old_token() {
        let _lock = TEST_MUTEX.lock().unwrap();
        reset_random();
        let mut manager = super::RetryManager::default();
        let token = manager.new_token();
        assert!(token.is_valid());
        manager.new_token();
        assert!(!token.is_valid());
    }

    // [utest->swdd~agent-workload-control-loop-prevents-retries-on-other-workload-commands~2]
    #[test]
    fn utest_new_token_stops_retry() {
        let _lock = TEST_MUTEX.lock().unwrap();
        reset_random();
        let mut manager = super::RetryManager::default();
        let token = manager.new_token();

        let (callback, called) = create_callback();

        let waker = waker();
        let mut context = Context::from_waker(&waker);

        let mut call_res = pin!(token.call_with_backoff(callback));
        assert_eq!(call_res.as_mut().poll(&mut context), Poll::Pending);
        assert!(called.borrow().is_none());

        manager.new_token();

        assert_eq!(call_res.as_mut().poll(&mut context), Poll::Ready(()));
        assert!(called.borrow().is_none());
    }
}
