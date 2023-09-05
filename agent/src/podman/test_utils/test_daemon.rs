// Copyright (c) 2023 Elektrobit Automotive GmbH
//
// This program and the accompanying materials are made available under the
// terms of the Apache License, Version 2.0 which is available at
// https://www.apache.org/licenses/LICENSE-2.0.
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
// WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
// License for the specific language governing permissions and limitations
// under the License.
//
// SPDX-License-Identifier: Apache-2.0

use std::{
    error::Error,
    fs,
    path::Path,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use rand::Rng;
use serde_json::Value;
use tokio::{sync::oneshot, task::JoinHandle};

use hyper::{
    service::{make_service_fn, service_fn},
    Body, Client, Response, Server, StatusCode,
};
use hyperlocal::{UnixClientExt, UnixServerExt, Uri};

use super::request_handlers::{ErrorResponseRequestHandler, RequestHandler};

pub struct PodmanTestDaemon {
    handle: JoinHandle<Result<(), Box<dyn Error + Send + Sync>>>,
    pub socket_path: String,
    request_handlers: Arc<Mutex<Vec<Box<dyn RequestHandler + Send + Sync>>>>,
    request_is_done_list: Vec<oneshot::Receiver<()>>,
}

impl PodmanTestDaemon {
    // Intentionally private. The daemon shall be created by the create() function.
    // The class itself stores an internal information.
    fn new(
        handle: JoinHandle<Result<(), Box<dyn Error + Send + Sync>>>,
        socket_path: String,
        request_handlers: Arc<Mutex<Vec<Box<dyn RequestHandler + Send + Sync>>>>,
    ) -> PodmanTestDaemon {
        let mut request_handler_lock = request_handlers.lock().unwrap();
        let mut request_is_done_list = Vec::with_capacity(request_handler_lock.len());
        for i in 0..request_handler_lock.len() {
            let v = &mut request_handler_lock[i];
            match v.get_expected_call_count() {
                Some(expected_call_count) if expected_call_count > 0 => {
                    let (s, r) = oneshot::channel();
                    v.set_is_done(s);
                    request_is_done_list.push(r);
                }
                _ => {}
            }
        }
        drop(request_handler_lock);

        PodmanTestDaemon {
            handle,
            socket_path,
            request_handlers,
            request_is_done_list,
        }
    }

    pub async fn create(
        request_handlers: Vec<Box<dyn RequestHandler + Sync + Send>>,
    ) -> PodmanTestDaemon {
        let socket_path = PodmanTestDaemon::generate_socket_name();
        log::debug!(
            "Creating the podman test daemon at socket '{}'",
            &socket_path
        );
        let request_handlers_arc = Arc::new(Mutex::new(request_handlers));
        let cloned_request_handlers_arc = request_handlers_arc.clone();
        let socket_path_clone = socket_path.clone();
        let handle = tokio::spawn(async move {
            PodmanTestDaemon::run(socket_path_clone, cloned_request_handlers_arc).await
        });

        let test_daemon = PodmanTestDaemon::new(handle, socket_path, request_handlers_arc);
        test_daemon.wait_daemon_till_ready().await;
        test_daemon
    }

    fn generate_socket_name() -> String {
        let socket_path_prefix = "/tmp/podman-test";
        let socket_path_suffix = ".sock";
        let mut rng = rand::thread_rng();
        format!(
            "{socket_path_prefix}-{}{socket_path_suffix}",
            rng.gen::<u32>()
        )
    }

    async fn run(
        socket_path: String,
        request_handlers: Arc<Mutex<Vec<Box<dyn RequestHandler + Sync + Send>>>>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        log::info!(
            "Starting the podman test daemon at socket '{}'",
            &socket_path
        );
        let path = Path::new(&socket_path);
        if path.exists() {
            fs::remove_file(path)?;
        }
        let make_service = make_service_fn(move |_| {
            let request_handlers = request_handlers.clone();
            async move {
                Ok::<_, hyper::Error>(service_fn(move |req| {
                    log::debug!(
                        "Received '{}' request with the uri: '{}'",
                        req.method(),
                        req.uri(),
                    );
                    let request_handlers = request_handlers.clone();
                    async move {
                        let uri = req.uri().to_string();
                        let method = req.method().to_string();

                        let body = hyper::body::to_bytes(req.into_body())
                            .await
                            .unwrap_or_default();
                        let body = std::str::from_utf8(&body).unwrap_or_default();
                        let body =
                            serde_json::from_str::<serde_json::Value>(body).unwrap_or_default();

                        let resp =
                            PodmanTestDaemon::handle_request(uri, method, request_handlers, body);
                        log::debug!("Answering with{:?}", resp);
                        Ok::<_, hyper::Error>(resp)
                    }
                }))
            }
        });
        Server::bind_unix(path)?.serve(make_service).await?;
        Ok(())
    }

    fn handle_request(
        req_uri: String,
        method: String,
        request_handlers: Arc<Mutex<Vec<Box<dyn RequestHandler + Sync + Send>>>>,
        body: Value,
    ) -> Response<Body> {
        let not_found_request_handler = ErrorResponseRequestHandler::default()
            .status_code(StatusCode::NOT_IMPLEMENTED)
            .error_message("Error: unknown request")
            .build();

        match request_handlers
            .lock()
            .unwrap()
            .iter_mut()
            .find(|x| x.is_the_handler(&req_uri, &method))
        {
            Some(request_handler) => {
                request_handler.add_call(body);
                request_handler.build()
            }
            None => not_found_request_handler,
        }
    }

    pub async fn wait_daemon_till_ready(&self) {
        let uri: hyper::Uri = Uri::new(&self.socket_path, "/initialtest").into();
        let client = Client::unix();
        while (client.get(uri.clone()).await).is_err() {
            log::debug!(
                "The daemon '{}' is not ready yet. Trying to connect in a while.",
                &self.socket_path
            );
            thread::sleep(Duration::from_millis(10));
        }
        log::debug!(
            "The daemon '{}' is ready to get a connection. The test can continue.",
            &self.socket_path
        );
    }

    pub fn check_calls_and_stop(&self) {
        self.request_handlers
            .lock()
            .unwrap()
            .iter_mut()
            .for_each(|x| x.check_calls());
        log::debug!("Trying to stop the thread");
        self.handle.abort();
    }

    pub async fn wait_expected_requests_done(&mut self) {
        for single_request_is_done in &mut self.request_is_done_list {
            single_request_is_done.await.unwrap();
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

mod tests {
    use hyper::{Client, StatusCode};
    use hyperlocal::{UnixClientExt, Uri};

    use crate::podman::test_utils::{
        request_handlers::{
            ErrorResponseRequestHandler, RequestHandler, WithRequestHandlerParameter,
        },
        test_daemon::PodmanTestDaemon,
    };

    #[tokio::test(flavor = "multi_thread")]
    async fn utest_daemon_unknown_request() {
        let _ = env_logger::builder().is_test(true).try_init();

        let request_handler = ErrorResponseRequestHandler::default()
            .request_path("/test")
            .status_code(StatusCode::INTERNAL_SERVER_ERROR)
            .error_message("A test request");

        let dummy_request_handler =
            vec![Box::new(request_handler) as Box<dyn RequestHandler + Sync + Send>];

        let test_daemon = PodmanTestDaemon::create(dummy_request_handler).await;

        let url = Uri::new(&test_daemon.socket_path, "/unknown").into();
        let client = Client::unix();
        match client.get(url).await {
            Ok(res) => {
                assert_eq!(res.status(), StatusCode::NOT_IMPLEMENTED);
            }
            Err(e) => panic!("Sending test request failed with error: {:?}", e),
        };

        test_daemon.check_calls_and_stop();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn utest_daemon_unknown_request_empty_daemon() {
        let _ = env_logger::builder().is_test(true).try_init();

        let empty_request_handler = Vec::new();

        let test_daemon = PodmanTestDaemon::create(empty_request_handler).await;

        let url = Uri::new(&test_daemon.socket_path, "/unknown").into();
        let client = Client::unix();
        match client.get(url).await {
            Ok(res) => {
                assert_eq!(res.status(), StatusCode::NOT_IMPLEMENTED);
            }
            Err(e) => panic!("Sending test request failed with error: {:?}", e),
        };

        test_daemon.check_calls_and_stop();
    }
}
