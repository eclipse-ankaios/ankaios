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

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc::Sender;
use tonic::Status;

use api::proto::ExecutionRequest;

type ShareableHashMap<K, V> = Arc<Mutex<HashMap<K, V>>>;

#[derive(Debug, Clone)]
pub struct AgentSendersMap {
    agent_senders: ShareableHashMap<String, Sender<Result<ExecutionRequest, Status>>>,
}

// Beside improving readability by hiding the lock steps, this trait helps improve the
// performance as it uses only the sync std::sync::Mutex.
// The get function here returns a clone of the sender eliminating the need to wait on
// the lock across the await of the send() of the sender.
//
// https://tokio.rs/tokio/tutorial/shared-state#on-using-stdsyncmutex
impl AgentSendersMap {
    pub fn new() -> Self {
        AgentSendersMap {
            agent_senders: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn get(&self, name: &str) -> Option<Sender<Result<ExecutionRequest, Status>>> {
        self.agent_senders.lock().unwrap().get(name).cloned()
    }

    pub fn insert(&self, name: &str, sender: Sender<Result<ExecutionRequest, Status>>) {
        self.agent_senders
            .lock()
            .unwrap()
            .insert(name.to_owned(), sender)
            .map_or_else(
                || {
                    log::debug!("Successfully added a new agent sender.");
                },
                |_replaced| {
                    log::warn!(
                        "Received a NEW hello from agent {name}. Replacing sender for this agent."
                    )
                },
            );
    }

    pub fn get_all_agent_names(&self) -> Vec<String> {
        self.agent_senders.lock().unwrap().keys().cloned().collect()
    }

    pub fn remove(&self, name: &str) {
        self.agent_senders.lock().unwrap().remove(name);
    }
}

impl Default for AgentSendersMap {
    fn default() -> Self {
        AgentSendersMap::new()
    }
}
