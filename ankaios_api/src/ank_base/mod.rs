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

pub(crate) use crate::helpers::{
    constrained_config_map, constrained_map_schema, serialize_to_ordered_map,
    tag_adapter_deserializer,
};

pub(crate) mod workload_instance_name;
pub use workload_instance_name::{
    ConfigHash, INSTANCE_NAME_SEPARATOR, WorkloadInstanceNameBuilder,
};

pub use file::{FileContent, FileContentSpec};

pub(crate) mod control_interface_access;
pub use access_rights_rule::AccessRightsRuleEnumSpec;
pub use control_interface_access::WILDCARD_SYMBOL;

pub(crate) mod workload;
pub use workload::{DeleteCondition, DeletedWorkload, FulfilledBy, WorkloadNamed, validate_tags};

pub(crate) mod workload_state;
pub use execution_state::{ExecutionStateEnum, ExecutionStateEnumSpec};

pub(crate) mod workload_states_map;

pub use config_item::{ConfigItemEnum, ConfigItemEnumSpec};

pub(crate) mod complete_state;

pub use request::{RequestContent, RequestContentSpec};
pub use response::ResponseContent;

pub(crate) mod requests;
