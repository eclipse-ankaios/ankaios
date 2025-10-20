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

use std::collections::HashSet;
use std::fmt::{self, Display};

pub type AgentName = String;
pub type WorkloadName = String;
pub type CliConnectionName = String;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CliRequestId {
    pub cli_name: CliConnectionName,
    pub request_uuid: String,
}

impl Display for CliRequestId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}@{}", self.cli_name, self.request_uuid)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AgentRequestId {
    pub agent_name: AgentName,
    pub workload_name: WorkloadName,
    pub request_uuid: String,
}

pub fn to_string_ids<I>(request_ids: HashSet<I>) -> HashSet<String>
where
    I: Display,
{
    request_ids.into_iter().map(|id| id.to_string()).collect()
}

impl Display for AgentRequestId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}@{}@{}",
            self.agent_name, self.workload_name, self.request_uuid
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RequestId {
    CliRequestId(CliRequestId),
    AgentRequestId(AgentRequestId),
}

impl Display for RequestId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RequestId::CliRequestId(cli_request_id) => {
                write!(f, "CLI request Id: {cli_request_id}")
            }
            RequestId::AgentRequestId(agent_request_id) => {
                write!(f, "agent request Id: {agent_request_id}")
            }
        }
    }
}

const CLI_PREFIX: &str = "cli-conn";
const CLI_REQUEST_PARTS_LEN: usize = 2;
const CLI_REQUEST_NAME_INDEX: usize = 0;
const CLI_REQUEST_ID_INDEX: usize = 1;

const AGENT_REQUEST_PARTS_LEN: usize = 3;
const AGENT_REQUEST_NAME_INDEX: usize = 0;
const AGENT_REQUEST_WORKLOAD_NAME_INDEX: usize = 1;
const AGENT_REQUEST_ID_INDEX: usize = 2;

impl<S> From<S> for RequestId
where
    S: AsRef<str>,
{
    fn from(request_id: S) -> Self {
        if request_id.as_ref().starts_with(CLI_PREFIX) {
            let parts: Vec<&str> = request_id
                .as_ref()
                .splitn(CLI_REQUEST_PARTS_LEN, '@')
                .collect();
            RequestId::CliRequestId(CliRequestId {
                cli_name: parts[CLI_REQUEST_NAME_INDEX].to_string(),
                request_uuid: parts[CLI_REQUEST_ID_INDEX].to_string(),
            })
        } else {
            let parts: Vec<&str> = request_id
                .as_ref()
                .splitn(AGENT_REQUEST_PARTS_LEN, '@')
                .collect();
            RequestId::AgentRequestId(AgentRequestId {
                agent_name: parts[AGENT_REQUEST_NAME_INDEX].to_string(),
                workload_name: parts[AGENT_REQUEST_WORKLOAD_NAME_INDEX].to_string(),
                request_uuid: parts[AGENT_REQUEST_ID_INDEX].to_string(),
            })
        }
    }
}
