// Copyright (c) 2026 Elektrobit Automotive GmbH
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

//! Example mutating hook for Ankaios.
//!
//! This hook processes workloads received via protobuf on stdin and:
//! - Rejects any workload tagged with `example_mutating_hook: reject`
//!   (exits non-zero with an error on stderr).
//! - Mutates the tag `example_mutating_hook: update` to
//!   `example_mutating_hook: marked` and returns the modified workload.
//! - Accepts all other workloads unchanged.

use prost::Message;
use std::io::{self, Read, Write};
use std::process;

#[allow(unused)]
mod grpc_api {
    include!(concat!(env!("OUT_DIR"), "/grpc_api.rs"));
}

#[allow(unused)]
mod ank_base {
    include!(concat!(env!("OUT_DIR"), "/ank_base.rs"));
}

const TAG_KEY: &str = "example_mutating_hook";

fn main() {
    let mut input = Vec::new();
    io::stdin()
        .read_to_end(&mut input)
        .expect("Failed to read stdin");

    let mut update = grpc_api::UpdateWorkload::decode(input.as_slice())
        .expect("Failed to decode UpdateWorkload from stdin");

    // Check all added workloads for the tag
    for added in &update.added_workloads {
        if let Some(workload) = &added.workload {
            if let Some(tags) = &workload.tags {
                if let Some(value) = tags.tags.get(TAG_KEY) {
                    if value == "reject" {
                        let name = added
                            .instance_name
                            .as_ref()
                            .map(|n| n.workload_name.as_str())
                            .unwrap_or("<unknown>");
                        eprintln!(
                            "Reject input workloads due to the presence of \
                             \"example_mutating_hook: reject\" tag on workload '{name}'"
                        );
                        process::exit(1);
                    }
                }
            }
        }
    }

    // Mutate: change "update" -> "marked"
    let mut mutated = false;
    for added in &mut update.added_workloads {
        if let Some(workload) = &mut added.workload {
            if let Some(tags) = &mut workload.tags {
                if let Some(value) = tags.tags.get_mut(TAG_KEY) {
                    if value == "update" {
                        *value = "marked".to_string();
                        mutated = true;
                    }
                }
            }
        }
    }

    // Write the (possibly mutated) result to stdout
    let output = update.encode_to_vec();
    if mutated {
        // Always write if we mutated so the server picks up the change
        io::stdout()
            .write_all(&output)
            .expect("Failed to write to stdout");
    } else {
        // Echo back the original input so the server sees no diff
        io::stdout()
            .write_all(&input)
            .expect("Failed to write to stdout");
    }
}
