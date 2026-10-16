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

use crate::grpc_api::{FromServer, ToServer};
use common::{
    communications_error::CommunicationMiddlewareError,
    from_server_interface::FromServerInterfaceError, to_server_interface::ToServerError,
};

use std::fmt;
use tokio::sync::mpsc::error::SendError;

#[derive(Debug, Clone)]
pub enum GrpcMiddlewareError {
    StartError(String),
    ReceiveError(String),
    SendError(String),
    ConversionError(String),
    ServerNotAvailable(String),
    ConnectionInterrupted(String),
    CertificateError(String),
    TLSError(String),
    VersionMismatch(String),
}

impl From<GrpcMiddlewareError> for CommunicationMiddlewareError {
    fn from(error: GrpcMiddlewareError) -> Self {
        CommunicationMiddlewareError(error.to_string())
    }
}

impl From<FromServerInterfaceError> for GrpcMiddlewareError {
    fn from(error: FromServerInterfaceError) -> Self {
        GrpcMiddlewareError::SendError(error.to_string())
    }
}

impl From<ToServerError> for GrpcMiddlewareError {
    fn from(error: ToServerError) -> Self {
        GrpcMiddlewareError::SendError(error.to_string())
    }
}

impl From<SendError<ToServer>> for GrpcMiddlewareError {
    fn from(error: SendError<ToServer>) -> Self {
        GrpcMiddlewareError::SendError(error.to_string())
    }
}

impl From<SendError<Result<FromServer, tonic::Status>>> for GrpcMiddlewareError {
    fn from(error: SendError<Result<FromServer, tonic::Status>>) -> Self {
        GrpcMiddlewareError::SendError(error.to_string())
    }
}

impl From<tonic::Status> for GrpcMiddlewareError {
    fn from(err: tonic::Status) -> Self {
        match err.code() {
            tonic::Code::FailedPrecondition => {
                GrpcMiddlewareError::VersionMismatch(err.to_string())
            }
            _ => GrpcMiddlewareError::ConnectionInterrupted(err.to_string()),
        }
    }
}

// [impl->swdd~grpc-client-outputs-error-server-unavailability-for-cli-connection~1]
impl From<tonic::transport::Error> for GrpcMiddlewareError {
    fn from(err: tonic::transport::Error) -> Self {
        let mut previous = err.to_string();
        let mut message = previous.clone();
        let mut source = std::error::Error::source(&err);

        while let Some(cause) = source {
            let detail = cause.to_string();
            if detail != previous {
                message.push_str(": ");
                message.push_str(&detail);
            }
            previous = detail;
            source = cause.source();
        }

        GrpcMiddlewareError::ServerNotAvailable(message)
    }
}

impl std::error::Error for GrpcMiddlewareError {}

impl fmt::Display for GrpcMiddlewareError {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            GrpcMiddlewareError::StartError(message) => write!(f, "StartError: '{message}'"),
            GrpcMiddlewareError::ReceiveError(message) => write!(f, "ReceiveError: '{message}'"),
            GrpcMiddlewareError::SendError(message) => write!(f, "SendError: '{message}'"),
            GrpcMiddlewareError::ConversionError(message) => {
                write!(f, "ConversionError: '{message}'")
            }
            GrpcMiddlewareError::ServerNotAvailable(message) => {
                write!(f, "Could not connect to the server: '{message}'")
            }
            GrpcMiddlewareError::ConnectionInterrupted(message) => {
                write!(f, "Connection interrupted: '{message}'")
            }
            GrpcMiddlewareError::CertificateError(message) => {
                write!(f, "Certificate error: '{message}'")
            }
            GrpcMiddlewareError::TLSError(message) => {
                write!(f, "TLS error: '{message}'")
            }
            GrpcMiddlewareError::VersionMismatch(message) => {
                write!(f, "Version mismatch: '{message}'")
            }
        }
    }
}

////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::GrpcMiddlewareError;
    use hyper_util::rt::TokioIo;
    use std::{error::Error, fmt, io, time::Duration};
    use tokio::io::DuplexStream;
    use tonic::transport::Endpoint;
    use tower::service_fn;

    const TEST_ENDPOINT_URI: &str = "http://unused";
    const INVALID_USER_AGENT: &str = "\n";
    const CONNECTOR_TIMEOUT: Duration = Duration::from_secs(5);
    const CONNECTION_FAILURE_MESSAGE: &str = "connection failed";
    const ACCESS_DENIED_MESSAGE: &str = "access denied";

    // [utest->swdd~grpc-client-outputs-error-server-unavailability-for-cli-connection~1]
    #[test]
    fn utest_transport_error_without_source() {
        let error = Endpoint::from_static(TEST_ENDPOINT_URI)
            .user_agent(INVALID_USER_AGENT)
            .unwrap_err();
        assert!(error.source().is_none());

        let converted = GrpcMiddlewareError::from(error);

        assert!(matches!(
            converted,
            GrpcMiddlewareError::ServerNotAvailable(message)
                if message == "user agent is not a valid header value"
        ));
    }

    // [utest->swdd~grpc-client-outputs-error-server-unavailability-for-cli-connection~1]
    #[tokio::test]
    async fn utest_transport_error_preserves_permission_denied_without_duplicates() {
        let error = connector_error(|| io::Error::from(io::ErrorKind::PermissionDenied)).await;
        let detail = io::Error::from(io::ErrorKind::PermissionDenied).to_string();

        let converted = GrpcMiddlewareError::from(error);

        assert!(matches!(
            &converted,
            GrpcMiddlewareError::ServerNotAvailable(message)
                if message == &format!("transport error: {detail}")
        ));
        let public_error =
            common::communications_error::CommunicationMiddlewareError::from(converted);
        assert_eq!(
            public_error.to_string(),
            format!("Could not connect to the server: 'transport error: {detail}'")
        );
    }

    // [utest->swdd~grpc-client-outputs-error-server-unavailability-for-cli-connection~1]
    #[tokio::test]
    async fn utest_transport_error_preserves_distinct_causes_after_duplicate() {
        let error = connector_error(|| TestError {
            message: CONNECTION_FAILURE_MESSAGE,
            source: Some(Box::new(TestError {
                message: CONNECTION_FAILURE_MESSAGE,
                source: Some(Box::new(TestError {
                    message: ACCESS_DENIED_MESSAGE,
                    source: None,
                })),
            })),
        })
        .await;

        let converted = GrpcMiddlewareError::from(error);

        assert!(matches!(
            converted,
            GrpcMiddlewareError::ServerNotAvailable(message)
                if message == format!(
                    "transport error: {CONNECTION_FAILURE_MESSAGE}: {ACCESS_DENIED_MESSAGE}"
                )
        ));
    }

    async fn connector_error<E>(
        make_error: impl Fn() -> E + Send + 'static,
    ) -> tonic::transport::Error
    where
        E: Error + Send + Sync + 'static,
    {
        tokio::time::timeout(
            CONNECTOR_TIMEOUT,
            Endpoint::from_static(TEST_ENDPOINT_URI).connect_with_connector(service_fn(
                move |_| std::future::ready(Err::<TokioIo<DuplexStream>, _>(make_error())),
            )),
        )
        .await
        .expect("connector failure must not hang")
        .unwrap_err()
    }

    #[derive(Debug)]
    struct TestError {
        message: &'static str,
        source: Option<Box<Self>>,
    }

    impl fmt::Display for TestError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(self.message)
        }
    }

    impl Error for TestError {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            self.source.as_deref().map(|source| source as &dyn Error)
        }
    }
}
