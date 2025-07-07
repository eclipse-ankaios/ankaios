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

use common::request_id_prepending::detach_prefix_from_request_id;
use std::collections::{HashMap, HashSet};

type AgentName = String;
pub type LogSubscriberRequestId = String;
type AgentLogRequestIdMap = HashMap<AgentName, HashSet<LogSubscriberRequestId>>;
type CliConnectionName = String;
type CliConnectionLogRequestIdMap = HashMap<CliConnectionName, LogSubscriberRequestId>;

const CLI_PREFIX: &str = "cli-conn";

#[derive(Default)]
pub struct LogCampaignStore {
    agent_log_request_ids_store: AgentLogRequestIdMap,
    cli_log_request_id_store: CliConnectionLogRequestIdMap,
}

#[cfg_attr(test, mockall::automock)]
impl LogCampaignStore {
    pub fn insert_log_campaign(&mut self, request_id: LogSubscriberRequestId) {
        let (agent_or_cli_name, _) = detach_prefix_from_request_id(&request_id);

        if agent_or_cli_name.starts_with(CLI_PREFIX) {
            let cli_connection_name = agent_or_cli_name;
            log::debug!(
                "Log campaign from CLI '{}', request id: '{}'",
                cli_connection_name,
                request_id
            );
            self.cli_log_request_id_store
                .insert(cli_connection_name, request_id.to_owned());
        } else {
            let agent_name = agent_or_cli_name;

            log::debug!(
                "Log campaign from agent '{}', request id: {}",
                agent_name,
                request_id
            );

            self.agent_log_request_ids_store
                .entry(agent_name)
                .or_default()
                .insert(request_id);
        }
    }

    pub fn remove_agent_log_campaign_entry(
        &mut self,
        agent_name: &AgentName,
    ) -> Option<HashSet<LogSubscriberRequestId>> {
        self.agent_log_request_ids_store.remove(agent_name)
    }

    pub fn remove_cli_log_campaign_entry(
        &mut self,
        cli_connection_name: &CliConnectionName,
    ) -> Option<LogSubscriberRequestId> {
        self.cli_log_request_id_store.remove(cli_connection_name)
    }

    pub fn remove_logs_request_id(&mut self, request_id: &LogSubscriberRequestId) {
        if request_id.starts_with(CLI_PREFIX) {
            self.cli_log_request_id_store
                .retain(|_cli_connection_name, cli_request_id| {
                    if cli_request_id == request_id {
                        log::debug!("Removing CLI log campaign with request id '{}' from log campaign store.", request_id);
                        false
                    } else {
                        true
                    }
                });
        } else {
            self.agent_log_request_ids_store
                .retain(|_agent_name, request_ids| {
                    if request_ids.remove(request_id) {
                        log::debug!("Removed workload log campaign with request id '{}' from log campaign store.", request_id);
                    }
                    !request_ids.is_empty()
                });
        }
    }
}

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////
#[cfg(test)]
mod tests {
    use super::{HashMap, HashSet, LogCampaignStore};

    const AGENT_A: &str = "agent_A";
    const REQUEST_ID_AGENT_A: &str = "agent_A@request_id";
    const AGENT_B: &str = "agent_B";
    const REQUEST_ID_AGENT_B: &str = "agent_B@request_id";
    const CLI_CONNECTION_1: &str = "cli-conn-1";
    const CLI_REQUEST_ID_1: &str = "cli-conn-1@cli_request_id_1";
    const CLI_CONNECTION_2: &str = "cli-conn-2";
    const CLI_REQUEST_ID_2: &str = "cli-conn-2@cli_request_id_2";

    #[test]
    fn utest_agent_log_connection_store_remove_all_logs_request_ids_for_agent() {
        let mut log_campaign_store = LogCampaignStore::default();
        log_campaign_store.insert_log_campaign(REQUEST_ID_AGENT_A.to_owned());
        log_campaign_store.insert_log_campaign(REQUEST_ID_AGENT_B.to_owned());

        let removed_requests =
            log_campaign_store.remove_agent_log_campaign_entry(&AGENT_A.to_owned());

        assert_eq!(
            removed_requests,
            Some(HashSet::from([REQUEST_ID_AGENT_A.to_owned()]))
        );

        assert!(log_campaign_store
            .agent_log_request_ids_store
            .contains_key(AGENT_B));
    }

    #[test]
    fn utest_agent_log_connection_store_remove_request_id() {
        let mut log_campaign_store = LogCampaignStore {
            agent_log_request_ids_store: HashMap::from([
                (
                    AGENT_A.to_owned(),
                    HashSet::from([REQUEST_ID_AGENT_A.to_owned()]),
                ),
                (
                    AGENT_B.to_owned(),
                    HashSet::from([REQUEST_ID_AGENT_B.to_owned()]),
                ),
            ]),
            ..Default::default()
        };

        log_campaign_store.remove_logs_request_id(&REQUEST_ID_AGENT_A.to_string());

        assert_eq!(
            log_campaign_store.agent_log_request_ids_store.get(AGENT_B),
            Some(&HashSet::from([REQUEST_ID_AGENT_B.to_owned()]))
        );
    }

    #[test]
    fn utest_cli_log_connection_store_remove_cli_logs_request() {
        let mut log_campaign_store = LogCampaignStore::default();
        log_campaign_store.insert_log_campaign(CLI_REQUEST_ID_1.to_owned());
        log_campaign_store.insert_log_campaign(CLI_REQUEST_ID_2.to_owned());

        let removed_request =
            log_campaign_store.remove_cli_log_campaign_entry(&CLI_CONNECTION_1.to_owned());

        assert_eq!(removed_request, Some(CLI_REQUEST_ID_1.to_owned()));

        assert!(log_campaign_store
            .cli_log_request_id_store
            .contains_key(CLI_CONNECTION_2));
    }

    #[test]
    fn utest_cli_log_connection_store_remove_request_id() {
        let mut log_campaign_store = LogCampaignStore {
            cli_log_request_id_store: HashMap::from([
                (CLI_CONNECTION_1.to_owned(), CLI_REQUEST_ID_1.to_owned()),
                (CLI_CONNECTION_2.to_owned(), CLI_REQUEST_ID_2.to_owned()),
            ]),
            ..Default::default()
        };
        log_campaign_store.remove_logs_request_id(&CLI_REQUEST_ID_1.to_string());
        assert_eq!(
            log_campaign_store
                .cli_log_request_id_store
                .get(CLI_CONNECTION_2),
            Some(&CLI_REQUEST_ID_2.to_owned())
        );
    }
}
