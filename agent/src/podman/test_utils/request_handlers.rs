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

use std::{convert::Infallible, mem::take};

use futures_util::stream;

use hyper::{Body, Response, StatusCode};
use serde_json::Value;
use tokio::sync::oneshot;

use super::server_models::{
    ServerContainerCreate, ServerError, ServerListContainer, ServerPullImages,
};

type CallChecker = dyn Fn(usize, &Value) -> bool + Send + Sync;

#[derive(Default)]
pub struct BaseRequestHandler {
    calls: Vec<Value>,
    call_checker: Option<Box<CallChecker>>,
    expected_call_count: Option<usize>,
    req_path: Option<String>,
    methods: Option<Vec<String>>,
    is_done: Option<oneshot::Sender<()>>,
}

pub trait RequestHandler {
    fn get_req_path(&self) -> Option<&str> {
        self.get_base().req_path.as_deref()
    }

    fn get_methods(&self) -> Option<&Vec<String>> {
        self.get_base().methods.as_ref()
    }

    fn build(&mut self) -> Response<Body>;

    fn get_base(&self) -> &BaseRequestHandler;
    fn get_base_mut(&mut self) -> &mut BaseRequestHandler;

    fn get_calls(&self) -> &Vec<Value> {
        &self.get_base().calls
    }

    fn set_is_done(&mut self, is_done: oneshot::Sender<()>) {
        self.get_base_mut().is_done = Some(is_done);
    }

    fn add_call(&mut self, value: Value) {
        self.get_base_mut().calls.push(value);
        if self
            .get_expected_call_count()
            .is_some_and(|expected_call_count| expected_call_count == self.get_call_count())
        {
            if let Some(set_done) = take(&mut self.get_base_mut().is_done) {
                set_done.send(()).unwrap();
            }
        }
    }

    fn get_call_count(&self) -> usize {
        self.get_calls().len()
    }

    fn get_expected_call_count(&self) -> Option<usize> {
        self.get_base().expected_call_count
    }

    fn get_call_checker(&self) -> &Option<Box<CallChecker>> {
        &self.get_base().call_checker
    }

    fn is_the_handler(&self, new_req_path: &str, new_method: &String) -> bool {
        if let Some(req_path) = self.get_req_path() {
            if !new_req_path.contains(req_path) {
                return false;
            }
        }
        if let Some(methods) = self.get_methods() {
            if !methods.contains(new_method) {
                return false;
            }
        }
        true
    }

    fn set_expected_call_count(&mut self, expected_call_count: usize) {
        self.get_base_mut().expected_call_count = Some(expected_call_count);
    }

    fn set_req_path(&mut self, req_path: &str) {
        self.get_base_mut().req_path = Some(String::from(req_path));
    }

    fn set_methods(&mut self, methods: Vec<String>) {
        self.get_base_mut().methods = Some(methods);
    }

    fn set_call_checker(&mut self, check_calls: Box<CallChecker>) {
        self.get_base_mut().call_checker = Some(check_calls);
    }

    fn check_calls(&self) {
        // The expected call count is set, we should check it
        if let Some(expected_call_count) = self.get_expected_call_count() {
            assert_eq!(
                self.get_call_count(),
                expected_call_count,
                "Expected '{:?}' on address '{:?}' to be called {} times. Has been called {} times.",
                self.get_methods(),
                self.get_req_path(),
                expected_call_count,
                self.get_call_count(),
            )
        }

        if let Some(call_checker) = self.get_call_checker() {
            for (index, call) in self.get_calls().iter().enumerate() {
                assert!(
                    call_checker(index, call),
                    "Check failed for call number {} with body {}",
                    index,
                    call
                );
            }
        }
    }
}

pub trait WithRequestHandlerParameter {
    fn times(self, expected_call_count: usize) -> Self;
    fn methods(self, methods: Vec<String>) -> Self;
    fn request_path(self, req_path: &str) -> Self;
    fn call_checker<F: Fn(usize, &Value) -> bool + Send + Sync + 'static>(
        self,
        check_calls: F,
    ) -> Self;
}

impl<T: RequestHandler> WithRequestHandlerParameter for T {
    fn times(mut self, expected_call_count: usize) -> Self {
        self.set_expected_call_count(expected_call_count);
        self
    }

    fn methods(mut self, methods: Vec<String>) -> Self {
        self.set_methods(methods);
        self
    }

    fn request_path(mut self, req_path: &str) -> Self {
        self.set_req_path(req_path);
        self
    }

    fn call_checker<F: Fn(usize, &Value) -> bool + Send + Sync + 'static>(
        mut self,
        call_checker: F,
    ) -> Self {
        self.set_call_checker(Box::new(call_checker));
        self
    }
}

#[derive(Default)]
pub struct BasicRequestHandler {
    base: BaseRequestHandler,
    status_code: StatusCode,
    body: String,
}

impl BasicRequestHandler {
    pub fn status_code(mut self, status_code: StatusCode) -> Self {
        self.status_code = status_code;
        self
    }

    pub fn resp_body(mut self, body: &str) -> Self {
        self.body = String::from(body);
        self
    }
}

impl RequestHandler for BasicRequestHandler {
    fn get_base(&self) -> &BaseRequestHandler {
        &self.base
    }
    fn get_base_mut(&mut self) -> &mut BaseRequestHandler {
        &mut self.base
    }

    fn build(&mut self) -> Response<Body> {
        Response::builder()
            .status(self.status_code)
            .body(Body::from(self.body.to_owned()))
            .unwrap()
    }
}

pub struct ListContainerRequestHandler {
    base: BaseRequestHandler,
    resp_body: String,
}

impl ListContainerRequestHandler {
    pub fn resp_body(mut self, container_list: &Vec<ServerListContainer>) -> Self {
        self.resp_body = serde_json::to_string(container_list).unwrap();
        self
    }

    pub fn resp_body_as_str(mut self, resp_body: String) -> Self {
        self.resp_body = resp_body;
        self
    }
}

impl Default for ListContainerRequestHandler {
    fn default() -> Self {
        Self {
            base: Default::default(),
            resp_body: String::new(),
        }
        .methods(vec!["GET".into()])
        .request_path("/libpod/containers")
    }
}

impl RequestHandler for ListContainerRequestHandler {
    fn get_base(&self) -> &BaseRequestHandler {
        &self.base
    }
    fn get_base_mut(&mut self) -> &mut BaseRequestHandler {
        &mut self.base
    }

    fn build(&mut self) -> Response<Body> {
        Response::builder()
            .header("Content-Type", "application/json")
            .body(Body::from(self.resp_body.clone()))
            .unwrap()
    }
}

#[derive(Default)]
pub struct ErrorResponseRequestHandler {
    base: BaseRequestHandler,
    error_message: String,
    status_code: StatusCode,
}

impl ErrorResponseRequestHandler {
    pub fn status_code(mut self, status_code: StatusCode) -> Self {
        self.status_code = status_code;
        self
    }

    pub fn error_message(mut self, error_message: &str) -> Self {
        self.error_message = String::from(error_message);
        self
    }
}

impl RequestHandler for ErrorResponseRequestHandler {
    fn get_base(&self) -> &BaseRequestHandler {
        &self.base
    }
    fn get_base_mut(&mut self) -> &mut BaseRequestHandler {
        &mut self.base
    }

    fn build(&mut self) -> Response<Body> {
        let server_error_response = ServerError {
            cause: String::new(),
            message: self.error_message.clone(),
            response: self.status_code.as_u16() as i64,
        };

        Response::builder()
            .status(self.status_code)
            .header("Content-Type", "application/json")
            .body(Body::from(
                serde_json::to_string(&server_error_response).unwrap(),
            ))
            .unwrap()
    }
}
pub struct PullImagesRequestHandler {
    base: BaseRequestHandler,
    resp_body: String,
}

impl PullImagesRequestHandler {
    pub fn resp_body(mut self, image_id: &str) -> Self {
        self.resp_body = serde_json::to_string(&ServerPullImages {
            id: image_id.to_string(),
            images: vec![image_id.to_string()],
        })
        .unwrap();
        self
    }
}

impl Default for PullImagesRequestHandler {
    fn default() -> Self {
        Self {
            base: Default::default(),
            resp_body: String::new(),
        }
        .methods(vec!["POST".into()])
        .request_path("/libpod/images/pull")
    }
}

impl RequestHandler for PullImagesRequestHandler {
    fn get_base(&self) -> &BaseRequestHandler {
        &self.base
    }
    fn get_base_mut(&mut self) -> &mut BaseRequestHandler {
        &mut self.base
    }

    fn build(&mut self) -> Response<Body> {
        let body_eof = "\r\n";
        let json_str = format!("{}{}", self.resp_body, body_eof);
        let chunked_body = vec![json_str];
        let stream = stream::iter(chunked_body.into_iter().map(Result::<_, Infallible>::Ok));
        let body = Body::wrap_stream(stream);

        Response::builder().body(body).unwrap()
    }
}

pub struct ContainerCreateRequestHandler {
    base: BaseRequestHandler,
    resp_body: String,
    status_code: StatusCode,
}

impl ContainerCreateRequestHandler {
    pub fn status_code(mut self, status_code: StatusCode) -> Self {
        self.status_code = status_code;
        self
    }

    pub fn resp_body(mut self, cont_id: &str) -> Self {
        self.resp_body = serde_json::to_string(&ServerContainerCreate {
            id: cont_id.to_string(),
            warnings: vec![String::from("")],
        })
        .unwrap();
        self
    }
}

impl Default for ContainerCreateRequestHandler {
    fn default() -> Self {
        Self {
            base: Default::default(),
            resp_body: String::new(),
            status_code: StatusCode::default(),
        }
        .methods(vec!["POST".into()])
        .request_path("/libpod/containers/create")
    }
}

impl RequestHandler for ContainerCreateRequestHandler {
    fn get_base(&self) -> &BaseRequestHandler {
        &self.base
    }
    fn get_base_mut(&mut self) -> &mut BaseRequestHandler {
        &mut self.base
    }

    fn build(&mut self) -> Response<Body> {
        Response::builder()
            .status(self.status_code)
            .header("Content-Type", "application/json")
            .body(Body::from(self.resp_body.clone()))
            .unwrap()
    }
}

pub mod handler_helpers {
    use hyper::StatusCode;

    use super::{
        BasicRequestHandler, ErrorResponseRequestHandler, RequestHandler,
        WithRequestHandlerParameter,
    };

    pub fn stop_success_handler(container_id: &str) -> Box<dyn RequestHandler + Send + Sync> {
        Box::new(
            BasicRequestHandler::default()
                .request_path(&format!("/libpod/containers/{container_id}/stop"))
                .status_code(StatusCode::NO_CONTENT)
                .times(1),
        )
    }

    pub fn stop_error_handler(container_id: &str) -> Box<dyn RequestHandler + Send + Sync> {
        Box::new(
            ErrorResponseRequestHandler::default()
                .request_path(&format!("/libpod/containers/{container_id}/stop"))
                .status_code(StatusCode::INTERNAL_SERVER_ERROR)
                .error_message("Simulated rejection")
                .times(1),
        )
    }

    pub fn delete_success_handler(container_id: &str) -> Box<dyn RequestHandler + Send + Sync> {
        Box::new(
            BasicRequestHandler::default()
                .methods(vec!["DELETE".into()])
                .request_path(&format!("/libpod/containers/{container_id}"))
                .status_code(StatusCode::NO_CONTENT)
                .times(1),
        )
    }

    pub fn delete_not_called_handler(container_id: &str) -> Box<dyn RequestHandler + Send + Sync> {
        Box::new(
            BasicRequestHandler::default()
                .methods(vec!["DELETE".into()])
                .request_path(&format!("/libpod/containers/{container_id}"))
                .status_code(StatusCode::NO_CONTENT)
                .times(0),
        )
    }

    pub fn delete_error_handler(container_id: &str) -> Box<dyn RequestHandler + Send + Sync> {
        Box::new(
            ErrorResponseRequestHandler::default()
                .methods(vec!["DELETE".into()])
                .request_path(&format!("/libpod/containers/{container_id}"))
                .status_code(StatusCode::INTERNAL_SERVER_ERROR)
                .error_message("Simulated rejection")
                .times(1),
        )
    }
}
