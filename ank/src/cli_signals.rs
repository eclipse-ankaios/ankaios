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

use crate::output_debug;

use api::std_extensions::UnreachableResult;
use tokio::signal::unix::{SignalKind, signal};

pub struct SignalHandler;

impl SignalHandler {
    // [impl->swdd~cli-provides-termination-signal-handling~1]
    #[allow(dead_code)]
    pub async fn wait_for_signals() {
        let mut sigint_sig = signal(SignalKind::interrupt()).unwrap_or_unreachable();
        let mut sigterm_sig = signal(SignalKind::terminate()).unwrap_or_unreachable();
        let mut sigquit_sig = signal(SignalKind::quit()).unwrap_or_unreachable();
        let mut sighup_sig = signal(SignalKind::hangup()).unwrap_or_unreachable();
        tokio::select! {
            _ = sigint_sig.recv() => {
                output_debug!("Received signal SIGINT");
            }
            _ = sigterm_sig.recv() => {
                output_debug!("Received signal SIGTERM");
            }
            _ = sigquit_sig.recv() => {
                output_debug!("Received signal SIGQUIT");
            }
            _ = sighup_sig.recv() => {
                output_debug!("Received signal SIGHUP");
            }
        }
    }
}

#[cfg(test)]
mockall::mock! {
    pub SignalHandler {
        pub fn wait_for_signals() -> std::pin::Pin<Box<dyn std::future::Future<Output=()>>>;
    }
}
