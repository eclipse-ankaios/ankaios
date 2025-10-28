// Copyright (c) 2025 Elektrobit Automotive GmbH
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

// [impl->swdd~ank-base-provides-object-definitions~1]
tonic::include_proto!("ank_base"); // The string specified here must match the proto package name

pub(crate) mod workload_instance_name;
pub use workload_instance_name::{
    ConfigHash, INSTANCE_NAME_SEPARATOR, WorkloadInstanceNameBuilder,
};

#[path = "file.rs"]
pub(crate) mod file_internal; // Rename needed to avoid conflict with tonic generated module
pub use file::{FileContent, FileContentInternal};

pub(crate) mod agent_map;
pub use agent_map::AgentLoadStatus;

pub(crate) mod control_interface_access;
pub use access_rights_rule::AccessRightsRuleEnumInternal;
pub use control_interface_access::WILDCARD_SYMBOL;

pub(crate) mod workload;
pub use workload::{
    ALLOWED_SYMBOLS, DeleteCondition, DeletedWorkload, FulfilledBy, STR_RE_AGENT,
    STR_RE_CONFIG_REFERENCES, verify_workload_name_format,
};

pub(crate) mod workload_state;
pub use execution_state::ExecutionStateEnumInternal;

pub(crate) mod config;

pub use request::RequestContent;
pub use response::ResponseContent;

// Use a local error type and From conversion so `?` can convert
// prost::UnknownEnumValue into your function's error type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnknownEnumErr(pub prost::UnknownEnumValue);

impl From<prost::UnknownEnumValue> for UnknownEnumErr {
    fn from(v: prost::UnknownEnumValue) -> Self {
        UnknownEnumErr(v)
    }
}

impl std::fmt::Display for UnknownEnumErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "UnknownEnumValue({})", (self.0).0)
    }
}

impl std::error::Error for UnknownEnumErr {}

impl Response {
    pub fn access_denied(request_id: String) -> Response {
        Response {
            request_id,
            response_content: response::ResponseContent::Error(Error {
                message: "Access denied".into(),
            })
            .into(),
        }
    }
}

impl std::fmt::Display for RestartPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RestartPolicy::Never => write!(f, "Never"),
            RestartPolicy::OnFailure => write!(f, "OnFailure"),
            RestartPolicy::Always => write!(f, "Always"),
        }
    }
}
