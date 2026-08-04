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

//! Performance instrumentation. `perf!(...)` logs to the dedicated `perf` target
//! when the `perf-metrics` feature is enabled, and compiles to nothing otherwise.
//! Filter it at runtime with e.g. `RUST_LOG=info,perf=trace`.

#[cfg(feature = "perf-metrics")]
macro_rules! perf {
    ($($arg:tt)+) => { log::info!(target: "perf", $($arg)+) };
}

#[cfg(not(feature = "perf-metrics"))]
macro_rules! perf {
    ($($arg:tt)+) => {};
}
