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

pub struct MockAllContextSync {
    mutex: tokio::sync::Mutex<()>,
}

impl MockAllContextSync {
    pub fn new() -> Self {
        Self {
            mutex: tokio::sync::Mutex::new(()),
        }
    }

    // Shares the same mutex as `get_lock` so sync and async tests mutually exclude
    // each other when touching the same global mockall static-mock context.
    // An async-aware guard avoids clippy::await_holding_lock in tests that hold it across `.await`.
    pub async fn get_lock_async(&'_ self) -> tokio::sync::MutexGuard<'_, ()> {
        self.mutex.lock().await
    }

    // Blocks the current thread; must only be called outside of an async execution context.
    pub fn get_lock(&'_ self) -> tokio::sync::MutexGuard<'_, ()> {
        self.mutex.blocking_lock()
    }
}

impl Default for MockAllContextSync {
    fn default() -> Self {
        Self::new()
    }
}
