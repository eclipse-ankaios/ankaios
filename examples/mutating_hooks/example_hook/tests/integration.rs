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

use prost::Message;
use std::collections::HashMap;
use std::io::Write;
use std::process::{Command, Stdio};

#[allow(unused)]
mod grpc_api {
    include!(concat!(env!("OUT_DIR"), "/grpc_api.rs"));
}

#[allow(unused)]
mod ank_base {
    include!(concat!(env!("OUT_DIR"), "/ank_base.rs"));
}

fn hook_path() -> std::path::PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop(); // remove test binary name
    path.pop(); // remove "deps"
    path.push("example_hook");
    path
}

fn make_update(tag_key: &str, tag_value: &str) -> grpc_api::UpdateWorkload {
    let mut tags = HashMap::new();
    tags.insert(tag_key.to_string(), tag_value.to_string());
    grpc_api::UpdateWorkload {
        added_workloads: vec![grpc_api::AddedWorkload {
            instance_name: Some(ank_base::WorkloadInstanceName {
                workload_name: "test_wl".to_string(),
                agent_name: "agent_A".to_string(),
                id: "123".to_string(),
            }),
            workload: Some(ank_base::Workload {
                agent: Some("agent_A".to_string()),
                tags: Some(ank_base::Tags { tags }),
                ..Default::default()
            }),
        }],
        deleted_workloads: vec![],
    }
}

fn run_hook(input: &[u8]) -> std::process::Output {
    let mut child = Command::new(hook_path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start hook");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input)
        .expect("Failed to write stdin");
    child.wait_with_output().expect("Failed to wait for hook")
}

#[test]
fn reject_tag_causes_nonzero_exit() {
    let msg = make_update("example_mutating_hook", "reject");
    let out = run_hook(&msg.encode_to_vec());
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("example_mutating_hook: reject"),
        "stderr should mention the reject tag, got: {stderr}"
    );
}

#[test]
fn update_tag_is_mutated_to_marked() {
    let msg = make_update("example_mutating_hook", "update");
    let out = run_hook(&msg.encode_to_vec());
    assert!(out.status.success());
    let result = grpc_api::UpdateWorkload::decode(out.stdout.as_slice()).unwrap();
    let tag_val = result.added_workloads[0]
        .workload
        .as_ref()
        .unwrap()
        .tags
        .as_ref()
        .unwrap()
        .tags
        .get("example_mutating_hook")
        .unwrap();
    assert_eq!(tag_val, "marked");
}

#[test]
fn normal_workload_passes_through_unchanged() {
    let msg = make_update("some_other_tag", "value");
    let encoded = msg.encode_to_vec();
    let out = run_hook(&encoded);
    assert!(out.status.success());
    assert_eq!(out.stdout, encoded);
}
