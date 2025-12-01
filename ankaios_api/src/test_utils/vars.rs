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

//! This module contains common variables used across the test fixtures.
//!
//! These constants help unify the test data and make it easier to maintain.

pub const API_VERSION: &str = "v1";

pub const BUFFER_SIZE: usize = 20;
// pub const REQUEST_ID: &str = "request_id";
// pub const RUN_FOLDER: &str = "run/folder";

// The runtime workload ids match the runtime configs' hashes
pub const RUNTIME_CONFIGS: [&str; 2] = [
    "generalOptions: [\"--version\"]\ncommandOptions: [\"--network=host\"]\nimage: alpine:latest\ncommandArgs: [\"bash\"]\n",
    "image: ghcr.io/eclipse-ankaios/tests/sleepy:latest",
];
pub const WORKLOAD_IDS: [&str; 2] = [
    "404e2079115f592befb2c97fc2666aefc59a7309214828b18ff9f20f47a6ebed",
    "f54d78fd9c57d2ec8ee16bb5571410d8370979784b2ae0dc7b645f01d9e2ee21",
];

pub const WORKLOAD_NAMES: [&str; 3] = ["workload_A", "workload_B", "workload_C"];
pub const AGENT_NAMES: [&str; 2] = ["agent_A", "agent_B"];
pub const RUNTIME_NAMES: [&str; 2] = ["runtime_A", "runtime_B"];
