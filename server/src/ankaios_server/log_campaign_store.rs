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

use std::{
    collections::{HashMap, HashSet},
    fmt::{self, Display},
};

use super::WorkloadInstanceNameInternal;

type AgentName = String;
pub type LogCollectorRequestId = String;
type AgentLogRequestIdMap = HashMap<AgentName, HashSet<AgentRequestId>>;
type CliConnectionName = String;
type CliConnectionLogRequestIdMap = HashMap<CliConnectionName, HashSet<CliRequestId>>;
type WorkloadName = String;
type WorkloadNameRequestIdMap = HashMap<WorkloadName, HashSet<AgentRequestId>>;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CliRequestId {
    cli_name: CliConnectionName,
    request_uuid: String,
}

impl Display for CliRequestId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}@{}", self.cli_name, self.request_uuid)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct AgentRequestId {
    agent_name: AgentName,
    workload_name: WorkloadName,
    request_uuid: String,
}

fn to_string_ids<I>(request_ids: HashSet<I>) -> HashSet<LogCollectorRequestId>
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
enum RequestId {
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

#[derive(Default, Debug, Clone)]
pub struct RemovedLogRequests {
    pub collector_requests: HashSet<LogCollectorRequestId>,
    pub disconnected_log_providers: Vec<(LogCollectorRequestId, Vec<WorkloadInstanceNameInternal>)>,
}

#[derive(Default)]
pub struct LogCampaignStore {
    agent_log_request_ids_store: AgentLogRequestIdMap,
    workload_name_request_id_store: WorkloadNameRequestIdMap,
    log_providers_store:
        HashMap<AgentName, HashMap<LogCollectorRequestId, Vec<WorkloadInstanceNameInternal>>>,
    cli_log_request_id_store: CliConnectionLogRequestIdMap,
}

#[cfg_attr(test, mockall::automock)]
// [impl->swdd~server-log-campaign-store-holds-log-campaign-metadata~1]
impl LogCampaignStore {
    pub fn insert_log_campaign(
        &mut self,
        input_request_id: &LogCollectorRequestId,
        log_providers: &Vec<WorkloadInstanceNameInternal>,
    ) {
        let request_id: RequestId = input_request_id.into();
        log::debug!("Insert log campaign '{request_id}'");

        match request_id {
            RequestId::CliRequestId(cli_request_id) => {
                self.cli_log_request_id_store
                    .entry(cli_request_id.cli_name.clone())
                    .or_default()
                    .insert(cli_request_id);
            }
            RequestId::AgentRequestId(agent_request_id) => {
                self.workload_name_request_id_store
                    .entry(agent_request_id.workload_name.clone())
                    .or_default()
                    .insert(agent_request_id.clone());

                self.agent_log_request_ids_store
                    .entry(agent_request_id.agent_name.clone())
                    .or_default()
                    .insert(agent_request_id.clone());
            }
        }

        for workload_instance_name in log_providers {
            self.log_providers_store
                .entry(workload_instance_name.agent_name().to_string())
                .or_default()
                .entry(input_request_id.clone())
                .or_default()
                .push(workload_instance_name.clone());
        }
    }

    pub fn remove_agent_log_campaign_entry(
        &mut self,
        agent_name: &AgentName,
    ) -> RemovedLogRequests {
        let requests = self.agent_log_request_ids_store.remove(agent_name);

        if let Some(requests) = &requests {
            requests.iter().for_each(|agent_request_id| {
                self.workload_name_request_id_store
                    .remove(&agent_request_id.workload_name);

                self.remove_request_from_log_providers_store(&agent_request_id.to_string());
            });
        }

        let disconnected_log_providers = self
            .log_providers_store
            .remove(agent_name)
            .map(|provider_map| {
                provider_map
                    .into_iter()
                    .map(|(agent_request_id, workload_instances)| {
                        (agent_request_id.to_string(), workload_instances)
                    })
                    .collect()
            })
            .unwrap_or_default();

        RemovedLogRequests {
            collector_requests: to_string_ids(requests.unwrap_or_default()),
            disconnected_log_providers,
        }
    }

    pub fn remove_cli_log_campaign_entry(
        &mut self,
        cli_connection_name: &CliConnectionName,
    ) -> HashSet<LogCollectorRequestId> {
        let removed_request_ids = self.cli_log_request_id_store.remove(cli_connection_name);

        if let Some(removed_request_ids) = &removed_request_ids {
            removed_request_ids.iter().for_each(|cli_request_id| {
                self.remove_request_from_log_providers_store(&cli_request_id.to_string());
            });
        }

        to_string_ids(removed_request_ids.unwrap_or_default())
    }

    pub fn remove_logs_request_id(&mut self, input_request_id: &LogCollectorRequestId) {
        let request_id: RequestId = input_request_id.into();
        log::debug!("Remove log campaign '{request_id}'");

        self.remove_request_from_log_providers_store(input_request_id);

        match request_id {
            RequestId::CliRequestId(cli_request_id) => {
                self.remove_request_from_cli_log_campaign_store(&cli_request_id);
            }
            RequestId::AgentRequestId(agent_request_id) => {
                self.remove_request_from_agent_log_campaign_store(&agent_request_id);

                self.remove_request_from_workload_log_campaign_store(&agent_request_id);
            }
        }
    }

    pub fn remove_collector_campaign_entry(
        &mut self,
        workload_name: &WorkloadName,
    ) -> HashSet<LogCollectorRequestId> {
        log::debug!("Removing collector campaign for workload '{workload_name}'");

        let removed_request_ids = self.workload_name_request_id_store.remove(workload_name);
        if let Some(removed_request_ids) = &removed_request_ids {
            removed_request_ids.iter().for_each(|agent_request_id| {
                self.remove_request_from_agent_log_campaign_store(agent_request_id);
                self.remove_request_from_log_providers_store(&agent_request_id.to_string());
            });
        }

        to_string_ids(removed_request_ids.unwrap_or_default())
    }

    fn remove_request_from_agent_log_campaign_store(&mut self, agent_request_id: &AgentRequestId) {
        if let Some(requests) = self
            .agent_log_request_ids_store
            .get_mut(&agent_request_id.agent_name)
        {
            requests.remove(agent_request_id);
            if requests.is_empty() {
                self.agent_log_request_ids_store
                    .remove(&agent_request_id.agent_name);
            }
        }
    }

    fn remove_request_from_cli_log_campaign_store(&mut self, cli_request_id: &CliRequestId) {
        if let Some(requests) = self
            .cli_log_request_id_store
            .get_mut(&cli_request_id.cli_name)
        {
            requests.remove(cli_request_id);
            if requests.is_empty() {
                self.cli_log_request_id_store
                    .remove(&cli_request_id.cli_name);
            }
        }
    }

    fn remove_request_from_workload_log_campaign_store(
        &mut self,
        agent_request_id: &AgentRequestId,
    ) {
        if let Some(wl_map_entry) = self
            .workload_name_request_id_store
            .get_mut(&agent_request_id.workload_name)
        {
            wl_map_entry.remove(agent_request_id);
            if wl_map_entry.is_empty() {
                self.workload_name_request_id_store
                    .remove(&agent_request_id.workload_name);
            }
        }
    }

    fn remove_request_from_log_providers_store(
        &mut self,
        agent_request_id: &LogCollectorRequestId,
    ) {
        self.log_providers_store
            .retain(|_agent_name, provider_map| {
                provider_map.retain(|agent_request_id_key, _log_providers| {
                    agent_request_id_key != agent_request_id
                });
                !provider_map.is_empty()
            });
    }
}

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////

// [utest->swdd~server-log-campaign-store-holds-log-campaign-metadata~1]
#[cfg(test)]
mod tests {
    use super::{super::WorkloadInstanceNameInternal, HashMap, HashSet, LogCampaignStore};

    const AGENT_A: &str = "agent_A";
    const WORKLOAD_1_NAME: &str = "workload_1";
    const REQUEST_ID_AGENT_A: &str = "agent_A@workload_1@request_id";
    const AGENT_B: &str = "agent_B";
    const WORKLOAD_2_NAME: &str = "workload_2";
    const REQUEST_ID_AGENT_B: &str = "agent_B@workload_2@request_id";
    const CLI_CON_1: &str = "cli-conn-1";
    const CLI_REQUEST_ID_1: &str = "cli-conn-1@cli_request_id_1";
    const CLI_CON_2: &str = "cli-conn-2";
    const CLI_REQUEST_ID_2: &str = "cli-conn-2@cli_request_id_2";
    const CLI_1_REQUEST_ID_3: &str = "cli-conn-1@cli_request_id_3";

    mockall::lazy_static! {
        static ref WORKLOAD_3_INSTANCE_NAME: WorkloadInstanceNameInternal = WorkloadInstanceNameInternal::try_from("log_provider.some_uuid.agent_B").unwrap();

    }

    fn prepare_log_campaign_store() -> LogCampaignStore {
        LogCampaignStore {
            agent_log_request_ids_store: HashMap::from([
                (
                    AGENT_A.to_owned(),
                    HashSet::from([to_agent_request_id(REQUEST_ID_AGENT_A)]),
                ),
                (
                    AGENT_B.to_owned(),
                    HashSet::from([to_agent_request_id(REQUEST_ID_AGENT_B)]),
                ),
            ]),
            cli_log_request_id_store: HashMap::from([
                (
                    CLI_CON_1.to_owned(),
                    HashSet::from([to_cli_request_id(CLI_REQUEST_ID_1)]),
                ),
                (
                    CLI_CON_2.to_owned(),
                    HashSet::from([to_cli_request_id(CLI_REQUEST_ID_2)]),
                ),
            ]),
            workload_name_request_id_store: HashMap::from([
                (
                    WORKLOAD_1_NAME.to_owned(),
                    HashSet::from([to_agent_request_id(REQUEST_ID_AGENT_A)]),
                ),
                (
                    WORKLOAD_2_NAME.to_owned(),
                    HashSet::from([to_agent_request_id(REQUEST_ID_AGENT_B)]),
                ),
            ]),
            log_providers_store: HashMap::from([(
                AGENT_B.to_owned(),
                HashMap::from([
                    (
                        REQUEST_ID_AGENT_A.to_owned(),
                        vec![WORKLOAD_3_INSTANCE_NAME.clone()],
                    ),
                    (
                        REQUEST_ID_AGENT_B.to_owned(),
                        vec![WORKLOAD_3_INSTANCE_NAME.clone()],
                    ),
                    (
                        CLI_REQUEST_ID_1.to_owned(),
                        vec![WORKLOAD_3_INSTANCE_NAME.clone()],
                    ),
                    (
                        CLI_REQUEST_ID_2.to_owned(),
                        vec![WORKLOAD_3_INSTANCE_NAME.clone()],
                    ),
                ]),
            )]),
        }
    }

    fn to_agent_request_id(request_id: &str) -> super::AgentRequestId {
        let request_id = request_id.into();

        match request_id {
            super::RequestId::AgentRequestId(agent_request_id) => agent_request_id,
            _ => panic!("Expected an AgentRequestId"),
        }
    }

    fn to_cli_request_id(request_id: &str) -> super::CliRequestId {
        let request_id = request_id.into();

        match request_id {
            super::RequestId::CliRequestId(cli_request_id) => cli_request_id,
            _ => panic!("Expected a CliRequestId"),
        }
    }

    #[test]
    fn utest_request_id_from_string() {
        let cli_request_id = super::RequestId::from(CLI_REQUEST_ID_1);
        assert!(
            matches!(cli_request_id, super::RequestId::CliRequestId(super::CliRequestId { cli_name, request_uuid })
            if cli_name == CLI_CON_1 && request_uuid == "cli_request_id_1")
        );

        let agent_request_id = super::RequestId::from(REQUEST_ID_AGENT_A);
        assert!(
            matches!(agent_request_id, super::RequestId::AgentRequestId(super::AgentRequestId { agent_name, workload_name, request_uuid })
            if agent_name == AGENT_A && workload_name == WORKLOAD_1_NAME && request_uuid == "request_id")
        );

        let extra_part = "@extra@parts.with_strange#format";
        let agent_request_id = super::RequestId::from(format!("{REQUEST_ID_AGENT_A}{extra_part}"));
        assert!(
            matches!(agent_request_id, super::RequestId::AgentRequestId(super::AgentRequestId { agent_name, workload_name, request_uuid })
            if agent_name == AGENT_A && workload_name == WORKLOAD_1_NAME && request_uuid == format!("request_id{extra_part}"))
        );
    }

    #[test]
    fn utest_cli_log_connection_store_insert_log_campaign() {
        let mut log_campaign_store = LogCampaignStore::default();

        log_campaign_store.insert_log_campaign(
            &CLI_REQUEST_ID_1.to_owned(),
            &vec![WORKLOAD_3_INSTANCE_NAME.clone()],
        );
        assert_eq!(log_campaign_store.cli_log_request_id_store.len(), 1);
        assert_eq!(
            log_campaign_store.cli_log_request_id_store.get(CLI_CON_1),
            Some(&HashSet::from([to_cli_request_id(CLI_REQUEST_ID_1)]))
        );
        assert_eq!(log_campaign_store.log_providers_store.len(), 1);
        assert_eq!(
            log_campaign_store.log_providers_store.get(AGENT_B),
            Some(&HashMap::from([(
                CLI_REQUEST_ID_1.to_owned(),
                vec![WORKLOAD_3_INSTANCE_NAME.clone()]
            )]))
        );

        log_campaign_store.insert_log_campaign(
            &CLI_REQUEST_ID_2.to_owned(),
            &vec![WORKLOAD_3_INSTANCE_NAME.clone()],
        );
        assert_eq!(log_campaign_store.cli_log_request_id_store.len(), 2);
        assert_eq!(
            log_campaign_store.cli_log_request_id_store.get(CLI_CON_2),
            Some(&HashSet::from([to_cli_request_id(CLI_REQUEST_ID_2)]))
        );
        assert_eq!(log_campaign_store.log_providers_store.len(), 1);
        assert_eq!(
            log_campaign_store.log_providers_store.get(AGENT_B),
            Some(&HashMap::from([
                (
                    CLI_REQUEST_ID_1.to_owned(),
                    vec![WORKLOAD_3_INSTANCE_NAME.clone()]
                ),
                (
                    CLI_REQUEST_ID_2.to_owned(),
                    vec![WORKLOAD_3_INSTANCE_NAME.clone()]
                )
            ]))
        );

        log_campaign_store.insert_log_campaign(
            &CLI_1_REQUEST_ID_3.to_owned(),
            &vec![WORKLOAD_3_INSTANCE_NAME.clone()],
        );
        assert_eq!(log_campaign_store.cli_log_request_id_store.len(), 2);
        assert_eq!(
            log_campaign_store.cli_log_request_id_store.get(CLI_CON_1),
            Some(&HashSet::from([
                to_cli_request_id(CLI_REQUEST_ID_1),
                to_cli_request_id(CLI_1_REQUEST_ID_3)
            ]))
        );
        assert_eq!(log_campaign_store.log_providers_store.len(), 1);
        assert_eq!(
            log_campaign_store.log_providers_store.get(AGENT_B),
            Some(&HashMap::from([
                (
                    CLI_REQUEST_ID_1.to_owned(),
                    vec![WORKLOAD_3_INSTANCE_NAME.clone()]
                ),
                (
                    CLI_REQUEST_ID_2.to_owned(),
                    vec![WORKLOAD_3_INSTANCE_NAME.clone()]
                ),
                (
                    CLI_1_REQUEST_ID_3.to_owned(),
                    vec![WORKLOAD_3_INSTANCE_NAME.clone()]
                )
            ]))
        );

        assert_eq!(log_campaign_store.agent_log_request_ids_store.len(), 0);
        assert_eq!(log_campaign_store.workload_name_request_id_store.len(), 0);
    }

    #[test]
    fn utest_agent_log_connection_store_insert_log_campaign() {
        let mut log_campaign_store = LogCampaignStore::default();

        log_campaign_store.insert_log_campaign(
            &REQUEST_ID_AGENT_A.to_owned(),
            &vec![WORKLOAD_3_INSTANCE_NAME.clone()],
        );
        assert_eq!(log_campaign_store.agent_log_request_ids_store.len(), 1);
        assert_eq!(
            log_campaign_store.agent_log_request_ids_store.get(AGENT_A),
            Some(&HashSet::from([to_agent_request_id(REQUEST_ID_AGENT_A)]))
        );
        assert_eq!(
            log_campaign_store
                .workload_name_request_id_store
                .get(WORKLOAD_1_NAME),
            Some(&HashSet::from([to_agent_request_id(REQUEST_ID_AGENT_A)]))
        );
        assert_eq!(
            log_campaign_store.log_providers_store.get(AGENT_B),
            Some(&HashMap::from([(
                REQUEST_ID_AGENT_A.to_owned(),
                vec![WORKLOAD_3_INSTANCE_NAME.clone()]
            )]))
        );

        log_campaign_store.insert_log_campaign(
            &REQUEST_ID_AGENT_B.to_owned(),
            &vec![WORKLOAD_3_INSTANCE_NAME.clone()],
        );
        assert_eq!(log_campaign_store.agent_log_request_ids_store.len(), 2);
        assert_eq!(
            log_campaign_store.agent_log_request_ids_store.get(AGENT_B),
            Some(&HashSet::from([to_agent_request_id(REQUEST_ID_AGENT_B)]))
        );
        assert_eq!(
            log_campaign_store
                .workload_name_request_id_store
                .get(WORKLOAD_2_NAME),
            Some(&HashSet::from([to_agent_request_id(REQUEST_ID_AGENT_B)]))
        );
        assert_eq!(
            log_campaign_store.log_providers_store.get(AGENT_B),
            Some(&HashMap::from([
                (
                    REQUEST_ID_AGENT_A.to_owned(),
                    vec![WORKLOAD_3_INSTANCE_NAME.clone()]
                ),
                (
                    REQUEST_ID_AGENT_B.to_owned(),
                    vec![WORKLOAD_3_INSTANCE_NAME.clone()]
                )
            ]))
        );

        assert_eq!(log_campaign_store.log_providers_store.len(), 1);
        assert_eq!(log_campaign_store.cli_log_request_id_store.len(), 0);
    }

    #[test]
    fn utest_agent_log_connection_store_remove_all_logs_request_ids_for_agent_collecting() {
        let mut log_campaign_store = prepare_log_campaign_store();

        let removed_requests =
            log_campaign_store.remove_agent_log_campaign_entry(&AGENT_A.to_owned());

        assert_eq!(
            removed_requests.collector_requests,
            HashSet::from([REQUEST_ID_AGENT_A.to_owned()])
        );

        assert_eq!(removed_requests.disconnected_log_providers.len(), 0);

        assert_eq!(log_campaign_store.agent_log_request_ids_store.len(), 1);
        assert_eq!(
            log_campaign_store.agent_log_request_ids_store.get(AGENT_B),
            Some(&HashSet::from([to_agent_request_id(REQUEST_ID_AGENT_B)]))
        );

        assert_eq!(log_campaign_store.workload_name_request_id_store.len(), 1);
        assert_eq!(
            log_campaign_store
                .workload_name_request_id_store
                .get(WORKLOAD_2_NAME),
            Some(&HashSet::from([to_agent_request_id(REQUEST_ID_AGENT_B)]))
        );

        assert_eq!(
            log_campaign_store.log_providers_store.get(AGENT_B).unwrap(),
            &HashMap::from([
                (
                    REQUEST_ID_AGENT_B.to_owned(),
                    vec![WORKLOAD_3_INSTANCE_NAME.clone()]
                ),
                (
                    CLI_REQUEST_ID_1.to_owned(),
                    vec![WORKLOAD_3_INSTANCE_NAME.clone()]
                ),
                (
                    CLI_REQUEST_ID_2.to_owned(),
                    vec![WORKLOAD_3_INSTANCE_NAME.clone()]
                ),
            ])
        );

        assert_eq!(log_campaign_store.cli_log_request_id_store.len(), 2);
    }

    #[test]
    fn utest_agent_log_connection_store_remove_all_logs_request_ids_for_agent_providing_logs() {
        let mut log_campaign_store = prepare_log_campaign_store();

        let removed_requests =
            log_campaign_store.remove_agent_log_campaign_entry(&AGENT_B.to_owned());

        assert_eq!(
            removed_requests.collector_requests,
            HashSet::from([REQUEST_ID_AGENT_B.to_owned()])
        );

        // We expect only 3 here as agent_B is disconnected and there is no sense to send a logs stops response there
        assert_eq!(removed_requests.disconnected_log_providers.len(), 3);

        assert_eq!(log_campaign_store.agent_log_request_ids_store.len(), 1);
        assert_eq!(
            log_campaign_store.agent_log_request_ids_store.get(AGENT_A),
            Some(&HashSet::from([to_agent_request_id(REQUEST_ID_AGENT_A)]))
        );

        assert_eq!(log_campaign_store.workload_name_request_id_store.len(), 1);
        assert_eq!(
            log_campaign_store
                .workload_name_request_id_store
                .get(WORKLOAD_1_NAME),
            Some(&HashSet::from([to_agent_request_id(REQUEST_ID_AGENT_A)]))
        );

        assert!(!log_campaign_store.log_providers_store.contains_key(AGENT_B));

        assert_eq!(log_campaign_store.cli_log_request_id_store.len(), 2);
    }

    #[test]
    fn utest_agent_log_connection_store_remove_request_id() {
        let mut log_campaign_store = prepare_log_campaign_store();

        log_campaign_store.remove_logs_request_id(&REQUEST_ID_AGENT_A.to_string());

        assert_eq!(log_campaign_store.agent_log_request_ids_store.len(), 1);
        assert_eq!(
            log_campaign_store.agent_log_request_ids_store.get(AGENT_B),
            Some(&HashSet::from([to_agent_request_id(REQUEST_ID_AGENT_B)]))
        );

        assert_eq!(log_campaign_store.workload_name_request_id_store.len(), 1);
        assert_eq!(
            log_campaign_store
                .workload_name_request_id_store
                .get(WORKLOAD_2_NAME),
            Some(&HashSet::from([to_agent_request_id(REQUEST_ID_AGENT_B)]))
        );

        assert_eq!(
            log_campaign_store.log_providers_store.get(AGENT_B).unwrap(),
            &HashMap::from([
                (
                    REQUEST_ID_AGENT_B.to_owned(),
                    vec![WORKLOAD_3_INSTANCE_NAME.clone()]
                ),
                (
                    CLI_REQUEST_ID_1.to_owned(),
                    vec![WORKLOAD_3_INSTANCE_NAME.clone()]
                ),
                (
                    CLI_REQUEST_ID_2.to_owned(),
                    vec![WORKLOAD_3_INSTANCE_NAME.clone()]
                ),
            ])
        );

        assert_eq!(log_campaign_store.cli_log_request_id_store.len(), 2);
    }

    #[test]
    fn utest_cli_log_connection_store_remove_cli_logs_request() {
        let mut log_campaign_store = prepare_log_campaign_store();

        let removed_request =
            log_campaign_store.remove_cli_log_campaign_entry(&CLI_CON_1.to_owned());

        assert_eq!(
            removed_request,
            HashSet::from([CLI_REQUEST_ID_1.to_owned()])
        );

        assert_eq!(log_campaign_store.cli_log_request_id_store.len(), 1);
        assert_eq!(
            log_campaign_store.cli_log_request_id_store.get(CLI_CON_2),
            Some(&HashSet::from([to_cli_request_id(CLI_REQUEST_ID_2)]))
        );

        assert_eq!(
            log_campaign_store.log_providers_store.get(AGENT_B).unwrap(),
            &HashMap::from([
                (
                    REQUEST_ID_AGENT_A.to_owned(),
                    vec![WORKLOAD_3_INSTANCE_NAME.clone()]
                ),
                (
                    REQUEST_ID_AGENT_B.to_owned(),
                    vec![WORKLOAD_3_INSTANCE_NAME.clone()]
                ),
                (
                    CLI_REQUEST_ID_2.to_owned(),
                    vec![WORKLOAD_3_INSTANCE_NAME.clone()]
                ),
            ])
        );

        assert_eq!(log_campaign_store.agent_log_request_ids_store.len(), 2);
        assert_eq!(log_campaign_store.workload_name_request_id_store.len(), 2);
    }

    #[test]
    fn utest_cli_log_connection_store_remove_request_id() {
        let mut log_campaign_store = prepare_log_campaign_store();

        log_campaign_store.remove_logs_request_id(&CLI_REQUEST_ID_1.to_string());
        assert_eq!(log_campaign_store.cli_log_request_id_store.len(), 1);
        assert_eq!(
            log_campaign_store.cli_log_request_id_store.get(CLI_CON_2),
            Some(&HashSet::from([to_cli_request_id(CLI_REQUEST_ID_2)]))
        );
        assert_eq!(
            log_campaign_store.log_providers_store.get(AGENT_B).unwrap(),
            &HashMap::from([
                (
                    REQUEST_ID_AGENT_A.to_owned(),
                    vec![WORKLOAD_3_INSTANCE_NAME.clone()]
                ),
                (
                    REQUEST_ID_AGENT_B.to_owned(),
                    vec![WORKLOAD_3_INSTANCE_NAME.clone()]
                ),
                (
                    CLI_REQUEST_ID_2.to_owned(),
                    vec![WORKLOAD_3_INSTANCE_NAME.clone()]
                ),
            ])
        );

        assert_eq!(log_campaign_store.agent_log_request_ids_store.len(), 2);
        assert_eq!(log_campaign_store.workload_name_request_id_store.len(), 2);
    }

    #[test]
    fn utest_remove_collector_campaign_entry() {
        let mut log_campaign_store = prepare_log_campaign_store();
        let removed_ids =
            log_campaign_store.remove_collector_campaign_entry(&WORKLOAD_1_NAME.to_owned());
        assert_eq!(removed_ids, HashSet::from([REQUEST_ID_AGENT_A.to_owned()]));

        assert_eq!(log_campaign_store.workload_name_request_id_store.len(), 1);
        assert_eq!(
            log_campaign_store
                .workload_name_request_id_store
                .get(WORKLOAD_2_NAME),
            Some(&HashSet::from([to_agent_request_id(REQUEST_ID_AGENT_B)]))
        );

        assert_eq!(log_campaign_store.agent_log_request_ids_store.len(), 1);
        assert_eq!(
            log_campaign_store.agent_log_request_ids_store.get(AGENT_B),
            Some(&HashSet::from([to_agent_request_id(REQUEST_ID_AGENT_B)]))
        );

        assert_eq!(
            log_campaign_store.log_providers_store.get(AGENT_B).unwrap(),
            &HashMap::from([
                (
                    REQUEST_ID_AGENT_B.to_owned(),
                    vec![WORKLOAD_3_INSTANCE_NAME.clone()]
                ),
                (
                    CLI_REQUEST_ID_1.to_owned(),
                    vec![WORKLOAD_3_INSTANCE_NAME.clone()]
                ),
                (
                    CLI_REQUEST_ID_2.to_owned(),
                    vec![WORKLOAD_3_INSTANCE_NAME.clone()]
                ),
            ])
        );

        assert_eq!(log_campaign_store.cli_log_request_id_store.len(), 2);
    }
}
