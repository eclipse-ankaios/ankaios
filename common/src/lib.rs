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

pub const CHANNEL_CAPACITY: usize = 20;
pub const DEFAULT_SOCKET_ADDRESS: &str = "127.0.0.1:25551";
pub const DEFAULT_SERVER_ADDRESS: &str = "http[s]://127.0.0.1:25551";
pub const PATH_SEPARATOR: char = '.';
pub const ANKAIOS_VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod commands;
pub mod communications_client;
pub mod communications_error;
pub mod communications_server;
pub mod from_server_interface;
pub mod helpers;
pub use helpers::check_version_compatibility;
pub mod objects;
pub mod request_id_prepending;
pub mod state_manipulation;
pub mod std_extensions;
#[cfg(feature = "test_utils")]
pub mod test_utils;
pub mod to_server_interface;
