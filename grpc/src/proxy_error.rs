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
use std::fmt;

use common::execution_interface::ExecutionCommandError;
pub enum GrpcProxyError {
    StreamingError(tonic::Status),
    Abort(String),
}

impl From<tonic::Status> for GrpcProxyError {
    fn from(status: tonic::Status) -> Self {
        GrpcProxyError::StreamingError(status)
    }
}

impl From<ExecutionCommandError> for GrpcProxyError {
    fn from(value: ExecutionCommandError) -> Self {
        GrpcProxyError::Abort(value.to_string())
    }
}

// impl From<String> for GrpcProxyError {
//     fn from(value: String) -> Self {
//         GrpcProxyError::Abort(value)
//     }
// }

impl fmt::Display for GrpcProxyError {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            GrpcProxyError::StreamingError(status) => {
                write!(f, "StreamingError: '{}'", status)
            }
            GrpcProxyError::Abort(message) => write!(f, "Abort: '{}'", message),
        }
    }
}
