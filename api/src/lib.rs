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

pub const ANKAIOS_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const API_VERSION_0_1: &str = "v0.1";
pub const API_VERSION_1_0: &str = "v1";
pub const CURRENT_API_VERSION: &str = API_VERSION_1_0;
pub const PREVIOUS_API_VERSION: &str = API_VERSION_0_1;

pub mod control_api {
    // [impl->swdd~control-api-provides-control-interface-definitions~1]
    tonic::include_proto!("control_api"); // The string specified here must match the proto package name
}

pub mod ank_base;
mod helpers;
pub mod std_extensions;

#[cfg(any(feature = "test_utils", test))]
pub mod test_utils;
