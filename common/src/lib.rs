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

use semver::Version;
use std_extensions::IllegalStateResult;

pub const CHANNEL_CAPACITY: usize = 20;
pub const DEFAULT_SOCKET_ADDRESS: &str = "127.0.0.1:25551";
pub const DEFAULT_SERVER_ADDRESS: &str = "http[s]://127.0.0.1:25551";
pub const PATH_SEPARATOR: char = '.';
pub const ANKAIOS_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn check_version_compatibility(version: impl AsRef<str>) -> Result<(), String> {
    let ank_version = Version::parse(ANKAIOS_VERSION).unwrap_or_illegal_state();
    if let Ok(input_version) = Version::parse(version.as_ref()) {
        if ank_version.major == input_version.major &&
        // As we are at a 0 (zero) major version, we also require minor version equality
        ank_version.minor == input_version.minor
        {
            return Ok(());
        }
    } else {
        log::warn!(
            "Could not parse incoming string '{}' as semantic version.",
            version.as_ref()
        );
    };

    Err(format!(
        "Unsupported protocol version '{}'. Currently supported '{ANKAIOS_VERSION}'",
        version.as_ref()
    ))
}

pub mod commands;
pub mod communications_client;
pub mod communications_error;
pub mod communications_server;
pub mod from_server_interface;
pub mod helpers;
pub mod objects;
pub mod request_id_prepending;
pub mod state_manipulation;
pub mod std_extensions;
#[cfg(feature = "test_utils")]
pub mod test_utils;
pub mod to_server_interface;
