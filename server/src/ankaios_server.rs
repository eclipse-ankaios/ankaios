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

mod update_state;

use common::std_extensions::IllegalStateResult;
#[cfg(test)]
use tests::update_state_mock as update_state;
#[cfg(not(test))]
use update_state::update_state;

use common::commands::{CompleteState, CompleteStateRequest, Request};
use common::from_server_interface::FromServer;
use common::objects::State;
use common::{from_server_interface::AgentInterface, to_server_interface::ToServer};
use tokio::sync::mpsc::{channel, Receiver, Sender};

use crate::ankaios_server::update_state::prepare_update_workload;
use crate::state_manipulation::Object;
use crate::workload_state_db::WorkloadStateDB;

pub type StateChangeChannels = (Sender<ToServer>, Receiver<ToServer>);
pub type ExecutionChannels = (Sender<FromServer>, Receiver<FromServer>);

pub fn create_state_change_channels(capacity: usize) -> StateChangeChannels {
    channel::<ToServer>(capacity)
}
pub fn create_execution_channels(capacity: usize) -> ExecutionChannels {
    channel::<FromServer>(capacity)
}

pub struct AnkaiosServer {
    // [impl->swdd~server-uses-async-channels~1]
    receiver: Receiver<ToServer>,
    // [impl->swdd~communication-to-from-server-middleware~1]
    to_agents: Sender<FromServer>,
    current_complete_state: CompleteState,
    workload_state_db: WorkloadStateDB,
}

impl AnkaiosServer {
    pub fn new(receiver: Receiver<ToServer>, to_agents: Sender<FromServer>) -> Self {
        AnkaiosServer {
            receiver,
            to_agents,
            current_complete_state: CompleteState::default(),
            workload_state_db: WorkloadStateDB::default(),
        }
    }

    fn get_complete_state_by_field_mask(
        &self,
        request_complete_state: &CompleteStateRequest,
    ) -> Result<CompleteState, String> {
        let current_complete_state = CompleteState {
            current_state: self.current_complete_state.current_state.clone(),
            startup_state: self.current_complete_state.startup_state.clone(),
            workload_states: self.workload_state_db.get_all_workload_states(),
        };

        // [impl->swdd~server-filters-get-complete-state-result~1]
        if !request_complete_state.field_mask.is_empty() {
            let current_complete_state: Object =
                current_complete_state.try_into().unwrap_or_illegal_state();
            let mut return_state = Object::default();

            for field in &request_complete_state.field_mask {
                if let Some(value) = current_complete_state.get(&field.into()) {
                    return_state.set(&field.into(), value.to_owned())?;
                } else {
                    log::debug!(
                        concat!(
                        "Result for CompleteState incomplete, as requested field does not exist:\n",
                        "   field: {}"),
                        field
                    );
                    continue;
                };
            }

            return_state.try_into().map_err(|err: serde_yaml::Error| {
                format!("The result for CompleteState is invalid: '{}'", err)
            })
        } else {
            Ok(current_complete_state)
        }
    }

    pub async fn start(&mut self) {
        log::info!("Starting...");
        self.listen_to_agents().await
    }

    async fn listen_to_agents(&mut self) {
        log::debug!("Start listening to agents...");
        while let Some(state_change_command) = self.receiver.recv().await {
            match state_change_command {
                ToServer::AgentHello(method_obj) => {
                    log::info!("Received AgentHello from '{}'", method_obj.agent_name);

                    // Send this agent all workloads in the current state which are assigned to him
                    let added_workloads = self
                        .current_complete_state
                        .current_state
                        .workloads
                        .clone()
                        .into_values()
                        // [impl->swdd~agent-from-agent-field~1]
                        .filter(|workload_spec| workload_spec.agent.eq(&method_obj.agent_name))
                        .collect();

                    log::debug!(
                        "Sending initial UpdateWorkload to agent '{}' with added workloads: '{:?}'",
                        method_obj.agent_name,
                        added_workloads,
                    );

                    // [impl->swdd~server-sends-all-workloads-on-start~1]
                    self.to_agents
                        .update_workload(
                            added_workloads,
                            // It's a newly connected agent, no need to delete anything.
                            vec![],
                        )
                        .await
                        .unwrap_or_illegal_state();

                    // [impl->swdd~server-informs-a-newly-connected-agent-workload-states~1]
                    // [impl->swdd~server-sends-all-workload-states-on-agent-connect~1]
                    let workload_states = self
                        .workload_state_db
                        .get_workload_state_excluding_agent(&method_obj.agent_name);

                    if !workload_states.is_empty() {
                        log::debug!(
                            "Sending initial UpdateWorkloadState to agent '{}' with workload states: '{:?}'",
                            method_obj.agent_name,
                            workload_states,
                        );

                        self.to_agents
                            .update_workload_state(workload_states)
                            .await
                            .unwrap_or_illegal_state();
                    } else {
                        log::debug!("No workload states to send.");
                    }
                }
                ToServer::AgentGone(method_obj) => {
                    log::debug!("Received AgentGone from '{}'", method_obj.agent_name);
                    // [impl->swdd~server-set-workload-state-unknown-on-disconnect~1]
                    self.workload_state_db
                        .mark_all_workload_state_for_agent_unknown(&method_obj.agent_name);

                    // communicate the workload state changes to other agents
                    // [impl->swdd~server-distribute-workload-state-unknown-on-disconnect~1]
                    self.to_agents
                        .update_workload_state(
                            self.workload_state_db
                                .get_workload_state_for_agent(&method_obj.agent_name),
                        )
                        .await
                        .unwrap_or_illegal_state();
                }
                ToServer::Request(Request {
                    request_id,
                    request_content,
                }) => match request_content {
                    // [impl->swdd~server-provides-interface-get-complete-state~1]
                    // [impl->swdd~server-includes-id-in-control-interface-response~1]
                    common::commands::RequestContent::CompleteStateRequest(request_content) => {
                        log::debug!(
                            "Received RequestCompleteState with id '{}' and field mask: '{:?}'",
                            request_id,
                            request_content.field_mask
                        );

                        match self.get_complete_state_by_field_mask(&request_content) {
                            Ok(complete_state) => self
                                .to_agents
                                .complete_state(request_id, complete_state)
                                .await
                                .unwrap_or_illegal_state(),
                            Err(error) => {
                                log::error!("Failed to get complete state: '{}'", error);
                                self.to_agents
                                    .complete_state(
                                        request_id,
                                        common::commands::CompleteState {
                                            startup_state: State::default(),
                                            current_state: State::default(),
                                            workload_states: vec![],
                                        },
                                    )
                                    .await
                                    .unwrap_or_illegal_state();
                            }
                        }
                    }
                    // [impl->swdd~server-provides-update-current-state-interface~1]
                    common::commands::RequestContent::UpdateStateRequest(request_content) => {
                        log::debug!(
                            "Received UpdateState. State '{:?}', update mask '{:?}'",
                            request_content.state,
                            request_content.update_mask
                        );

                        match update_state(&self.current_complete_state, *request_content) {
                            Ok(new_state) => {
                                let cmd = prepare_update_workload(
                                    &self.current_complete_state.current_state,
                                    &new_state.current_state,
                                );

                                if let Some(cmd) = cmd {
                                    self.to_agents.send(cmd).await.unwrap_or_illegal_state();
                                } else {
                                    log::debug!("The current state and new state are identical -> nothing to do");
                                }
                                self.current_complete_state = new_state;
                            }
                            Err(error) => {
                                log::error!("Could not execute UpdateRequest: '{}'", error);
                            }
                        }
                    }
                },
                ToServer::UpdateWorkloadState(method_obj) => {
                    log::debug!(
                        "Received UpdateWorkloadState: '{:?}'",
                        method_obj.workload_states
                    );

                    // [impl->swdd~server-stores-workload-state~1]
                    self.workload_state_db
                        .insert(method_obj.workload_states.clone());

                    // [impl->swdd~server-forwards-workload-state~1]
                    self.to_agents
                        .update_workload_state(method_obj.workload_states)
                        .await
                        .unwrap_or_illegal_state();
                }

                ToServer::Stop(_method_obj) => {
                    log::debug!("Received Stop from communications server");
                    // TODO: handle the call
                    break;
                }
                unknown_message => {
                    log::warn!(
                        "Received an unknown message from communications server: '{:?}'",
                        unknown_message
                    );
                }
            }
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

    extern crate serde;
    use std::cell::RefCell;
    use std::collections::{HashMap, VecDeque};
    use std::future::Future;

    use common::commands::{CompleteStateRequest, Response, ResponseContent, UpdateStateRequest};
    use common::objects::{DeletedWorkload, State, Tag, WorkloadSpec, WorkloadState};
    use common::test_utils::generate_test_workload_spec_with_param;
    use common::{
        commands::CompleteState,
        from_server_interface::FromServer,
        to_server_interface::{ToServer, ToServerInterface},
    };
    use tokio::join;
    use tokio::sync::mpsc::{self, channel, Receiver, Sender};

    use super::update_state::UpdateStateError;
    use super::{create_execution_channels, create_state_change_channels, AnkaiosServer};

    type TestSetup = (
        (
            AnkaiosServer,
            tokio::task::JoinHandle<()>,
            tokio::task::JoinHandle<()>,
            tokio::task::JoinHandle<()>,
        ), // ( server instance, communication mapper task, fake agent 1 task, fake agent 2 task)
        (Sender<ToServer>, Sender<FromServer>), // (state change sender channel to ankaios server, execution sender channel to communication mapper)
        Receiver<TestResult>,                   // test result receiver channel
    );

    const RUNTIME_NAME: &str = "fake_runtime";

    #[derive(PartialEq, Debug, Clone)]
    enum TestResult {
        Result(String),
    }

    #[derive(Debug)]
    struct CommunicationMapper {
        fake_agents: HashMap<String, Sender<FromServer>>,
        ex_receiver: Receiver<FromServer>,
    }

    impl CommunicationMapper {
        fn new(ex_receiver: Receiver<FromServer>) -> Self {
            CommunicationMapper {
                fake_agents: HashMap::new(),
                ex_receiver,
            }
        }
        fn insert(&mut self, agent_name: String, to_agent: Sender<FromServer>) {
            self.fake_agents.insert(agent_name, to_agent);
        }

        async fn start(&mut self) {
            while let Some(ex_command) = self.ex_receiver.recv().await {
                match ex_command {
                    FromServer::UpdateWorkload(update_workload) => {
                        let agent_names: Vec<String> = update_workload
                            .added_workloads
                            .iter()
                            .map(|wl| wl.agent.clone())
                            .collect();

                        let relevant_agents: Vec<(String, Sender<FromServer>)> = self
                            .fake_agents
                            .clone()
                            .into_iter()
                            .filter(|x| agent_names.iter().any(|y| y == &x.0))
                            .collect();

                        for (_, agent_sender) in relevant_agents.into_iter() {
                            agent_sender
                                .send(FromServer::UpdateWorkload(update_workload.clone()))
                                .await
                                .unwrap();
                        }
                    }
                    FromServer::UpdateWorkloadState(update_workload_state) => {
                        let agent_names: Vec<String> = update_workload_state
                            .workload_states
                            .iter()
                            .map(|wls| wls.agent_name.clone())
                            .collect();

                        let relevant_agents: Vec<(String, Sender<FromServer>)> = self
                            .fake_agents
                            .clone()
                            .into_iter()
                            .filter(|x| agent_names.iter().any(|y| y == &x.0))
                            .collect();

                        for (_, agent_sender) in relevant_agents.into_iter() {
                            agent_sender
                                .send(FromServer::UpdateWorkloadState(
                                    update_workload_state.clone(),
                                ))
                                .await
                                .unwrap();
                        }
                    }
                    FromServer::Response(Response {
                        request_id,
                        response_content,
                    }) => {
                        let mut splitted = request_id.split('@');
                        let agent_name = splitted.next().unwrap();
                        let request_id = splitted.next().unwrap().to_owned();
                        let agent_sender = self.fake_agents.get(agent_name).unwrap();
                        agent_sender
                            .send(FromServer::Response(Response {
                                request_id,
                                response_content,
                            }))
                            .await
                            .unwrap();
                    }
                    _ => panic!(),
                }
            }
        }
    }

    struct FakeAgent {
        ex_receiver: Receiver<FromServer>,
        tc_sender: Sender<TestResult>,
    }

    impl FakeAgent {
        fn new(ex_receiver: Receiver<FromServer>, tc_sender: Sender<TestResult>) -> Self {
            FakeAgent {
                ex_receiver,
                tc_sender,
            }
        }

        async fn start<F, Fut>(&mut self, handler: F)
        where
            F: Fn(Sender<TestResult>, FromServer) -> Fut,
            Fut: Future<Output = ()>,
        {
            while let Some(ex_command) = self.ex_receiver.recv().await {
                handler(self.tc_sender.clone(), ex_command).await;
            }
        }
    }

    fn create_test_setup() -> TestSetup {
        //
        //  ________________                           _____________________                            ______________
        // | ankaios server |---ex_command_channel--->| CommunicationMapper |---ex_command_channel---> | fake agent N |
        // |                |                         |                     |                          |              |
        // |                |                         |_____________________|                          |              |
        // |________________|                                                                          |______________|
        //         ^                                                                                          |
        //         | state_change_channel                                                                     |
        //  _______|________                                                                                  |
        // |    Test Case   |                                                                                 |
        // |________________|<----------------------------test_result_channel----------------------------------
        //
        // Note: the fake agent only forwards commands received through the "ex_command_channel" to the Test Case.
        // This way the Test Case can check which execution commands the agent is supposed to receive.
        // If the agent in your Test Case needs to sent a status change command, it must be done by the Test Case itself
        // (the fake agent contains no business logic).

        // [utest->swdd~server-uses-async-channels~1]
        let (to_server, server_receiver) = create_state_change_channels(common::CHANNEL_CAPACITY);
        let (to_cm, cm_receiver) = create_execution_channels(common::CHANNEL_CAPACITY);
        let (to_agent_1, agent_1_receiver) = create_execution_channels(common::CHANNEL_CAPACITY);
        let (to_agent_2, agent_2_receiver) = create_execution_channels(common::CHANNEL_CAPACITY);
        let (to_test_case, test_case_receiver) = channel::<TestResult>(common::CHANNEL_CAPACITY);

        let ankaios_server = AnkaiosServer::new(server_receiver, to_cm.clone());

        let mut cm_server = CommunicationMapper::new(cm_receiver);
        cm_server.insert("fake_agent_1".to_owned(), to_agent_1);
        cm_server.insert("fake_agent_2".to_owned(), to_agent_2);
        let cm_server_task = tokio::spawn(async move { cm_server.start().await });

        let agent_ex_command_handler = |tcs: Sender<TestResult>, ex_command| async move {
            match ex_command {
                FromServer::UpdateWorkload(update_workload) => {
                    tcs.send(TestResult::Result(
                        serde_json::to_string(&update_workload).unwrap().to_owned(),
                    ))
                    .await
                    .unwrap();
                }
                FromServer::UpdateWorkloadState(update_workload_state) => tcs
                    .send(TestResult::Result(
                        serde_json::to_string(&update_workload_state)
                            .unwrap()
                            .to_owned(),
                    ))
                    .await
                    .unwrap(),
                FromServer::Response(response) => tcs
                    .send(TestResult::Result(
                        serde_json::to_string(&response).unwrap().to_owned(),
                    ))
                    .await
                    .unwrap(),

                _ => panic!(),
            }
        };

        let mut fake_agent_1 = FakeAgent::new(agent_1_receiver, to_test_case.clone());
        let fake_agent_1_task =
            tokio::spawn(async move { fake_agent_1.start(agent_ex_command_handler).await });

        let mut fake_agent_2 = FakeAgent::new(agent_2_receiver, to_test_case);
        let fake_agent_2_task =
            tokio::spawn(async move { fake_agent_2.start(agent_ex_command_handler).await });

        (
            (
                ankaios_server,
                cm_server_task,
                fake_agent_1_task,
                fake_agent_2_task,
            ),
            (to_server, to_cm),
            test_case_receiver,
        )
    }

    fn get_workloads(
        result_from_fake_agent: &TestResult,
    ) -> Option<common::commands::UpdateWorkload> {
        match result_from_fake_agent {
            TestResult::Result(update_workload) => match serde_json::from_str(update_workload) {
                Ok(wl) => Some(wl),
                Err(_) => None,
            },
        }
    }
    fn get_workload_states(
        result_from_fake_agent: &TestResult,
    ) -> Option<common::commands::UpdateWorkloadState> {
        match result_from_fake_agent {
            TestResult::Result(update_workload_state) => {
                match serde_json::from_str(update_workload_state) {
                    Ok(ws) => Some(ws),
                    Err(_) => None,
                }
            }
        }
    }

    fn get_response(result_from_fake_agent: &TestResult) -> Option<common::commands::Response> {
        match result_from_fake_agent {
            TestResult::Result(res) => match serde_json::from_str(res) {
                Ok(some_typed_object) => Some(some_typed_object),
                Err(_) => None,
            },
        }
    }

    fn check_update_workload(
        tested_workload: Option<common::commands::UpdateWorkload>,
        expected_agent_name: String,
        expected_workload_names: Vec<String>,
    ) {
        if let Some(wl) = tested_workload {
            let workload_names: Vec<(String, String)> = wl
                .added_workloads
                .into_iter()
                .map(|wls| (wls.agent, wls.name))
                .collect();

            let agent_name: &str = workload_names[0].0.as_ref();
            assert_eq!(expected_agent_name, agent_name);
            assert_eq!(expected_workload_names.len(), workload_names.len());
            let mut actual_workload_names: Vec<String> =
                workload_names.clone().into_iter().map(|x| x.1).collect();
            actual_workload_names.sort();
            assert_eq!(expected_workload_names, actual_workload_names);
        }
    }

    fn check_update_workload_state(
        tested_workload_state: Option<common::commands::UpdateWorkloadState>,
        expected_agent_name: String,
        expected_workload_states: Vec<WorkloadState>,
    ) {
        if let Some(ws) = tested_workload_state {
            assert_eq!(expected_agent_name, ws.workload_states[0].agent_name);
            assert_eq!(expected_workload_states.len(), ws.workload_states.len());
            let mut wls = ws.workload_states;
            wls.sort_by(|a, b| a.workload_name.cmp(&b.workload_name));
            assert_eq!(expected_workload_states, wls);
        }
    }

    pub fn update_state_mock(
        current_state: &CompleteState,
        update: UpdateStateRequest,
    ) -> Result<CompleteState, UpdateStateError> {
        UPDATE_STATE_MOCK_CALLS.with(move |calls| {
            let mut calls = calls.borrow_mut();
            calls.push_back((current_state.to_owned(), update));
        });

        UPDATE_STATE_MOCK_RESULTS.with(move |results| {
            let mut results = results.borrow_mut();
            results.pop_front().unwrap()
        })
    }

    thread_local! {
        static UPDATE_STATE_MOCK_CALLS: RefCell<VecDeque<(CompleteState,UpdateStateRequest)>>  = RefCell::new(VecDeque::new());
        static UPDATE_STATE_MOCK_RESULTS: RefCell<VecDeque<Result<CompleteState, UpdateStateError>>> = RefCell::new(VecDeque::new());
    }

    // [utest->swdd~server-sends-all-workloads-on-start~1]
    // [utest->swdd~agent-from-agent-field~1]
    #[tokio::test]
    async fn utest_server_sends_workloads_and_workload_states() {
        // prepare test setup
        let fake_agent_names = ["fake_agent_1", "fake_agent_2"];
        let (
            (mut ankaios_server, cm_server_task, fake_agent_1_task, fake_agent_2_task),
            (to_server, _to_cm_server),
            mut tc_receiver,
        ) = create_test_setup();

        // prepare workload specs
        let mut wl = HashMap::new();
        wl.insert(
            "fake_workload_spec_1".to_owned(),
            generate_test_workload_spec_with_param(
                fake_agent_names[0].to_owned(),
                "fake_workload_1".to_owned(),
                RUNTIME_NAME.to_string(),
            ),
        );
        wl.insert(
            "fake_workload_spec_2".to_owned(),
            generate_test_workload_spec_with_param(
                fake_agent_names[0].to_owned(),
                "fake_workload_2".to_owned(),
                RUNTIME_NAME.to_string(),
            ),
        );
        wl.insert(
            "fake_workload_spec_3".to_owned(),
            generate_test_workload_spec_with_param(
                fake_agent_names[1].to_owned(),
                "fake_workload_3".to_owned(),
                RUNTIME_NAME.to_string(),
            ),
        );

        // prepare current state
        ankaios_server.current_complete_state = CompleteState {
            current_state: State {
                workloads: wl,
                configs: HashMap::default(),
                cron_jobs: HashMap::default(),
            },
            ..Default::default()
        };

        let server_task = tokio::spawn(async move { ankaios_server.start().await });

        // fake_agent_1 connects to the ankaios server
        let agent_hello_result = to_server.agent_hello(fake_agent_names[0].to_owned()).await;
        assert!(agent_hello_result.is_ok());

        check_update_workload(
            get_workloads(&tc_receiver.recv().await.unwrap()),
            "fake_agent_1".to_string(),
            vec!["fake_workload_1".to_string(), "fake_workload_2".to_string()],
        );

        // [utest->swdd~server-informs-a-newly-connected-agent-workload-states~1]
        // [utest->swdd~server-sends-all-workload-states-on-agent-connect~1]
        // send update_workload_state for fake_agent_1 which is then stored in the workload_state_db in ankaios server
        let update_workload_state_result = to_server
            .update_workload_state(vec![
                common::objects::WorkloadState {
                    agent_name: fake_agent_names[0].to_string(),
                    workload_name: "fake_workload_1".to_string(),
                    execution_state: common::objects::ExecutionState::ExecRunning,
                },
                common::objects::WorkloadState {
                    agent_name: fake_agent_names[0].to_string(),
                    workload_name: "fake_workload_2".to_string(),
                    execution_state: common::objects::ExecutionState::ExecSucceeded,
                },
            ])
            .await;
        assert!(update_workload_state_result.is_ok());

        check_update_workload_state(
            get_workload_states(&tc_receiver.recv().await.unwrap()),
            "fake_agent_1".to_string(),
            vec![
                common::objects::WorkloadState {
                    agent_name: "fake_agent_1".to_string(),
                    workload_name: "fake_workload_1".to_string(),
                    execution_state: common::objects::ExecutionState::ExecRunning,
                },
                common::objects::WorkloadState {
                    agent_name: "fake_agent_1".to_string(),
                    workload_name: "fake_workload_2".to_string(),
                    execution_state: common::objects::ExecutionState::ExecSucceeded,
                },
            ],
        );

        // fake_agent_2 connects to the ankaios server
        let agent_hello_result = to_server.agent_hello(fake_agent_names[1].to_owned()).await;
        assert!(agent_hello_result.is_ok());

        check_update_workload(
            get_workloads(&tc_receiver.recv().await.unwrap()),
            "fake_agent_2".to_string(),
            vec!["fake_workload_3".to_string()],
        );

        check_update_workload_state(
            get_workload_states(&tc_receiver.recv().await.unwrap()),
            "fake_agent_1".to_string(),
            vec![
                common::objects::WorkloadState {
                    agent_name: "fake_agent_1".to_string(),
                    workload_name: "fake_workload_1".to_string(),
                    execution_state: common::objects::ExecutionState::ExecRunning,
                },
                common::objects::WorkloadState {
                    agent_name: "fake_agent_1".to_string(),
                    workload_name: "fake_workload_2".to_string(),
                    execution_state: common::objects::ExecutionState::ExecSucceeded,
                },
            ],
        );

        // [utest->swdd~server-forwards-workload-state~1]
        // send update_workload_state for fake_agent_2 which is then stored in the workload_state_db in ankaios server
        let update_workload_state_result = to_server
            .update_workload_state(vec![common::objects::WorkloadState {
                agent_name: fake_agent_names[1].to_string(),
                workload_name: "fake_workload_3".to_string(),
                execution_state: common::objects::ExecutionState::ExecSucceeded,
            }])
            .await;
        assert!(update_workload_state_result.is_ok());

        check_update_workload_state(
            get_workload_states(&tc_receiver.recv().await.unwrap()),
            "fake_agent_2".to_string(),
            vec![common::objects::WorkloadState {
                agent_name: "fake_agent_2".to_string(),
                workload_name: "fake_workload_3".to_string(),
                execution_state: common::objects::ExecutionState::ExecSucceeded,
            }],
        );

        // send update_workload_state for fake_agent_1 which is then stored in the workload_state_db in ankaios server
        let update_workload_state_result = to_server
            .update_workload_state(vec![
                common::objects::WorkloadState {
                    agent_name: fake_agent_names[0].to_string(),
                    workload_name: "fake_workload_1".to_string(),
                    execution_state: common::objects::ExecutionState::ExecSucceeded,
                },
                common::objects::WorkloadState {
                    agent_name: fake_agent_names[0].to_string(),
                    workload_name: "fake_workload_2".to_string(),
                    execution_state: common::objects::ExecutionState::ExecSucceeded,
                },
            ])
            .await;
        assert!(update_workload_state_result.is_ok());

        // for fake_agent_2 check reception of update_workload_state of fake_agent_1
        check_update_workload_state(
            get_workload_states(&tc_receiver.recv().await.unwrap()),
            "fake_agent_1".to_string(),
            vec![
                common::objects::WorkloadState {
                    agent_name: "fake_agent_1".to_string(),
                    workload_name: "fake_workload_1".to_string(),
                    execution_state: common::objects::ExecutionState::ExecSucceeded,
                },
                common::objects::WorkloadState {
                    agent_name: "fake_agent_1".to_string(),
                    workload_name: "fake_workload_2".to_string(),
                    execution_state: common::objects::ExecutionState::ExecSucceeded,
                },
            ],
        );

        // clean up
        fake_agent_1_task.abort();
        fake_agent_2_task.abort();
        cm_server_task.abort();
        server_task.abort();
    }

    // [utest->swdd~server-provides-update-current-state-interface~1]
    #[tokio::test]
    async fn utest_server_sends_workloads_and_workload_states_when_requested_update_state() {
        // prepare names
        let agent_names = ["fake_agent_1", "fake_agent_2"];
        let workload_names = ["workload_1", "workload_2"];
        let request_id = "id1";
        let update_mask = format!("workloads.{}", workload_names[1]);

        // prepare structures
        let workloads = vec![
            generate_test_workload_spec_with_param(
                agent_names[0].to_owned(),
                workload_names[0].to_owned(),
                RUNTIME_NAME.to_string(),
            ),
            generate_test_workload_spec_with_param(
                agent_names[1].to_owned(),
                workload_names[1].to_owned(),
                RUNTIME_NAME.to_string(),
            ),
        ];

        let original_state = CompleteState {
            current_state: State {
                workloads: vec![(workload_names[0].to_owned(), workloads[0].clone())]
                    .into_iter()
                    .collect(),
                configs: HashMap::default(),
                cron_jobs: HashMap::default(),
            },
            ..Default::default()
        };

        let update_state = CompleteState {
            current_state: State {
                workloads: vec![(workload_names[1].to_owned(), workloads[1].clone())]
                    .into_iter()
                    .collect(),
                configs: HashMap::default(),
                cron_jobs: HashMap::default(),
            },
            ..Default::default()
        };

        let expected_state = CompleteState {
            current_state: State {
                workloads: vec![
                    (workload_names[0].to_owned(), workloads[0].clone()),
                    (workload_names[1].to_owned(), workloads[1].clone()),
                ]
                .into_iter()
                .collect(),
                configs: HashMap::default(),
                cron_jobs: HashMap::default(),
            },
            ..Default::default()
        };

        let mock_result = expected_state.clone();

        // prepare mock
        UPDATE_STATE_MOCK_RESULTS.with(move |results| {
            let mut results = results.borrow_mut();
            results.clear();
            results.push_back(Ok(mock_result));
        });
        UPDATE_STATE_MOCK_CALLS.with(move |calls| {
            let mut calls = calls.borrow_mut();
            calls.clear();
        });

        // prepare test setup
        let (
            (mut ankaios_server, cm_server_task, fake_agent_1_task, fake_agent_2_task),
            (to_server, _to_cm_server),
            mut tc_receiver,
        ) = create_test_setup();

        // prepare current state
        ankaios_server.current_complete_state = original_state.clone();
        let server_task = tokio::spawn(async move { ankaios_server.start().await });

        let agent_hello_result = to_server.agent_hello(agent_names[0].to_owned()).await;
        assert!(agent_hello_result.is_ok());

        // send new state to server
        let update_state_result = to_server
            .update_state(
                format!("{}@{}", agent_names[0], request_id),
                update_state.clone(),
                vec![update_mask.clone()],
            )
            .await;
        assert!(update_state_result.is_ok());

        // request complete state
        let request_complete_state_result = to_server
            .request_complete_state(
                format!("{}@{}", agent_names[0], request_id),
                CompleteStateRequest { field_mask: vec![] },
            )
            .await;
        assert!(request_complete_state_result.is_ok());

        let _ignore_added_workloads = tc_receiver.recv().await;
        let complete_state = tc_receiver.recv().await;

        let expected_complete_state = Response {
            request_id: request_id.into(),
            response_content: ResponseContent::CompleteState(Box::new(CompleteState {
                startup_state: State {
                    workloads: HashMap::new(),
                    configs: HashMap::new(),
                    cron_jobs: HashMap::new(),
                },
                current_state: expected_state.current_state.clone(),
                workload_states: vec![],
            })),
        };

        assert_eq!(
            complete_state,
            Some(TestResult::Result(
                serde_json::to_string(&expected_complete_state).unwrap()
            ))
        );

        let actual_call = UPDATE_STATE_MOCK_CALLS.with(move |calls| {
            let mut calls = calls.borrow_mut();
            calls.pop_front().unwrap()
        });

        assert_eq!(
            actual_call,
            (
                original_state,
                UpdateStateRequest {
                    state: update_state,
                    update_mask: vec![update_mask]
                }
            )
        );

        fake_agent_1_task.abort();
        fake_agent_2_task.abort();
        cm_server_task.abort();
        server_task.abort();
    }

    // [utest->swdd~server-provides-interface-get-complete-state~1]
    // [utest->swdd~server-filters-get-complete-state-result~1]
    // [utest->swdd~server-includes-id-in-control-interface-response~1]
    #[tokio::test]
    async fn utest_server_returns_complete_state_when_received_request_complete_state() {
        // prepare test setup
        let agent_name_fake_agent_1: &str = "fake_agent_1";
        let (
            (mut ankaios_server, cm_server_task, fake_agent_1_task, _),
            (to_server, _to_cm_server),
            mut tc_receiver,
        ) = create_test_setup();

        // prepare workload specs
        let mut workloads = HashMap::new();
        workloads.insert(
            "fake_workload_spec_1".to_owned(),
            generate_test_workload_spec_with_param(
                agent_name_fake_agent_1.to_owned(),
                "fake_workload_1".to_owned(),
                RUNTIME_NAME.to_string(),
            ),
        );
        workloads.insert(
            "fake_workload_spec_2".to_owned(),
            generate_test_workload_spec_with_param(
                agent_name_fake_agent_1.to_owned(),
                "fake_workload_2".to_owned(),
                RUNTIME_NAME.to_string(),
            ),
        );
        workloads.insert(
            "fake_workload_spec_3".to_owned(),
            generate_test_workload_spec_with_param(
                agent_name_fake_agent_1.to_owned(),
                "fake_workload_3".to_owned(),
                RUNTIME_NAME.to_string(),
            ),
        );

        let mut configs = HashMap::new();
        configs.insert("key1".into(), "value1".into());
        configs.insert("key2".into(), "value2".into());
        configs.insert("key3".into(), "value3".into());

        let test_state = CompleteState {
            current_state: State {
                workloads,
                configs,
                cron_jobs: HashMap::default(),
            },
            ..Default::default()
        };

        // prepare current state
        ankaios_server.current_complete_state = test_state.clone();

        let server_task = tokio::spawn(async move { ankaios_server.start().await });

        let check_workload_state = |next_result: TestResult, expected_complete_state: &Response| {
            if let Some(response) = get_response(&next_result) {
                assert_eq!(expected_complete_state.request_id, response.request_id);
                let complete_state = if let ResponseContent::CompleteState(complete_state) =
                    response.response_content
                {
                    complete_state
                } else {
                    panic!("Response is not CompleteState");
                };

                let expected_complete_state =
                    if let ResponseContent::CompleteState(expected_complete_state) =
                        expected_complete_state.response_content.to_owned()
                    {
                        expected_complete_state
                    } else {
                        panic!("Exepected response is not CompleteState");
                    };

                assert_eq!(
                    expected_complete_state.current_state,
                    complete_state.current_state
                );
                assert_eq!(
                    expected_complete_state.startup_state,
                    complete_state.startup_state
                );
                assert_eq!(
                    expected_complete_state.workload_states,
                    complete_state.workload_states
                );
            }
        };

        let agent_hello_result = to_server
            .agent_hello(agent_name_fake_agent_1.to_owned())
            .await;
        assert!(agent_hello_result.is_ok());

        let _skip_hello_result_as_not_on_test_focus = tc_receiver.recv().await.unwrap();

        // send command 'RequestCompleteState' with empty field mask meaning without active filter
        // so CompleteState shall contain the complete state
        let request_complete_state_result = to_server
            .request_complete_state(
                format!("{agent_name_fake_agent_1}@my_request_id"),
                super::CompleteStateRequest { field_mask: vec![] },
            )
            .await;
        assert!(request_complete_state_result.is_ok());

        check_workload_state(
            tc_receiver.recv().await.unwrap(),
            &Response {
                request_id: String::from("my_request_id"),
                response_content: ResponseContent::CompleteState(Box::new(CompleteState {
                    startup_state: State::default(),
                    workload_states: vec![],
                    current_state: test_state.current_state.clone(),
                })),
            },
        );

        // send command 'RequestCompleteState' with field mask = ["workloadStates"]
        let request_complete_state_result = to_server
            .request_complete_state(
                format!("{agent_name_fake_agent_1}@my_request_id"),
                super::CompleteStateRequest {
                    field_mask: vec![
                        String::from("workloadStates"),
                        String::from("currentState.workloads.fake_workload_spec_1"),
                        String::from("currentState.workloads.fake_workload_spec_3.tags"),
                        String::from("currentState.workloads.fake_workload_spec_4"),
                    ],
                },
            )
            .await;
        assert!(request_complete_state_result.is_ok());

        check_workload_state(
            tc_receiver.recv().await.unwrap(),
            &Response {
                request_id: String::from("my_request_id"),
                response_content: ResponseContent::CompleteState(Box::new(CompleteState {
                    current_state: State {
                        workloads: vec![
                            (
                                "fake_workload_spec_1".into(),
                                test_state
                                    .current_state
                                    .workloads
                                    .get("fake_workload_spec_1")
                                    .unwrap()
                                    .to_owned(),
                            ),
                            (
                                "fake_workload_spec_3".into(),
                                WorkloadSpec {
                                    tags: vec![Tag {
                                        key: "key".into(),
                                        value: "value".into(),
                                    }],
                                    ..Default::default()
                                },
                            ),
                        ]
                        .into_iter()
                        .collect(),
                        ..Default::default()
                    },
                    ..Default::default()
                })),
            },
        );

        // clean up
        fake_agent_1_task.abort();
        cm_server_task.abort();
        server_task.abort();
    }

    #[tokio::test]
    async fn utest_start_distributes_workload_unknown_after_disconnect() {
        let _ = env_logger::builder().is_test(true).try_init();

        const BUFFER_SIZE: usize = 20;
        let fake_agent_names = ["fake_agent_1", "fake_agent_2"];

        let (to_agents, mut agents_receiver) = mpsc::channel::<FromServer>(BUFFER_SIZE);
        let (to_server, server_receiver) = mpsc::channel::<ToServer>(BUFFER_SIZE);

        let mut server = AnkaiosServer::new(server_receiver, to_agents);

        // prepare workload specs
        let mut wl = HashMap::new();
        wl.insert(
            "fake_workload_spec_1".to_owned(),
            generate_test_workload_spec_with_param(
                fake_agent_names[0].to_owned(),
                "fake_workload_1".to_owned(),
                RUNTIME_NAME.to_string(),
            ),
        );
        wl.insert(
            "fake_workload_spec_2".to_owned(),
            generate_test_workload_spec_with_param(
                fake_agent_names[0].to_owned(),
                "fake_workload_2".to_owned(),
                RUNTIME_NAME.to_string(),
            ),
        );
        wl.insert(
            "fake_workload_spec_3".to_owned(),
            generate_test_workload_spec_with_param(
                fake_agent_names[1].to_owned(),
                "fake_workload_3".to_owned(),
                RUNTIME_NAME.to_string(),
            ),
        );

        // prepare current state
        server.current_complete_state = CompleteState {
            current_state: State {
                workloads: wl,
                configs: HashMap::default(),
                cron_jobs: HashMap::default(),
            },
            ..Default::default()
        };

        let agent_hello1_result = to_server.agent_hello(fake_agent_names[0].to_owned()).await;
        assert!(agent_hello1_result.is_ok());
        let agent_hello2_result = to_server.agent_hello(fake_agent_names[1].to_owned()).await;
        assert!(agent_hello2_result.is_ok());

        // send update_workload_state for fake_agent_1 which is then stored in the workload_state_db in ankaios server
        let update_workload_state_result = to_server
            .update_workload_state(vec![
                common::objects::WorkloadState {
                    agent_name: fake_agent_names[0].to_string(),
                    workload_name: "fake_workload_1".to_string(),
                    execution_state: common::objects::ExecutionState::ExecRunning,
                },
                common::objects::WorkloadState {
                    agent_name: fake_agent_names[0].to_string(),
                    workload_name: "fake_workload_2".to_string(),
                    execution_state: common::objects::ExecutionState::ExecSucceeded,
                },
            ])
            .await;
        assert!(update_workload_state_result.is_ok());

        // fake_agent_1 disconnects from the ankaios server
        let agent_gone_result = to_server.agent_gone(fake_agent_names[0].to_owned()).await;
        assert!(agent_gone_result.is_ok());

        let handle = server.start();

        // The receiver in the server receives the messages and terminates the infinite waiting-loop
        drop(to_server);
        join!(handle);

        // [utest->swdd~server-stores-workload-state~1]
        // [utest->swdd~server-set-workload-state-unknown-on-disconnect~1]
        let mut workload_states = server
            .workload_state_db
            .get_workload_state_for_agent(fake_agent_names[0]);

        workload_states.sort_by(|a, b| a.workload_name.cmp(&b.workload_name));

        assert_eq!(workload_states.len(), 2);
        assert_eq!(
            workload_states,
            vec![
                common::objects::WorkloadState {
                    agent_name: "fake_agent_1".to_string(),
                    workload_name: "fake_workload_1".to_string(),
                    execution_state: common::objects::ExecutionState::ExecUnknown,
                },
                common::objects::WorkloadState {
                    agent_name: "fake_agent_1".to_string(),
                    workload_name: "fake_workload_2".to_string(),
                    execution_state: common::objects::ExecutionState::ExecUnknown,
                },
            ]
        );

        // UpdateWorkload for the Fake Agent 1
        let agent_message = agents_receiver.try_recv();
        assert!(agent_message.is_ok());
        match agent_message.unwrap() {
            FromServer::UpdateWorkload(wl) => {
                check_update_workload(
                    Some(wl),
                    "fake_agent_1".to_string(),
                    vec!["fake_workload_1".to_string(), "fake_workload_2".to_string()],
                );
            }
            cmd => panic!("Unexpected command {:?}", cmd),
        }

        // UpdateWorkload for the Fake Agent 2
        let agent_message = agents_receiver.try_recv();
        assert!(agent_message.is_ok());
        match agent_message.unwrap() {
            FromServer::UpdateWorkload(wl) => {
                check_update_workload(
                    Some(wl),
                    "fake_agent_2".to_string(),
                    vec!["fake_workload_3".to_string()],
                );
            }
            cmd => panic!("Unexpected command {:?}", cmd),
        }

        // UpdateWorkloadState for the Fake Agent 2
        let agent_message = agents_receiver.try_recv();
        assert!(agent_message.is_ok());
        match agent_message.unwrap() {
            FromServer::UpdateWorkloadState(wls) => {
                check_update_workload_state(
                    Some(wls),
                    "fake_agent_1".to_string(),
                    vec![
                        common::objects::WorkloadState {
                            agent_name: "fake_agent_1".to_string(),
                            workload_name: "fake_workload_1".to_string(),
                            execution_state: common::objects::ExecutionState::ExecRunning,
                        },
                        common::objects::WorkloadState {
                            agent_name: "fake_agent_1".to_string(),
                            workload_name: "fake_workload_2".to_string(),
                            execution_state: common::objects::ExecutionState::ExecSucceeded,
                        },
                    ],
                );
            }
            cmd => panic!("Unexpected command {:?}", cmd),
        }

        // UpdateWorkloadState for the Fake Agent 2
        // [utest->swdd~server-distribute-workload-state-unknown-on-disconnect~1]
        let agent_message = agents_receiver.try_recv();
        assert!(agent_message.is_ok());
        match agent_message.unwrap() {
            FromServer::UpdateWorkloadState(wls) => {
                check_update_workload_state(
                    Some(wls),
                    "fake_agent_1".to_string(),
                    vec![
                        common::objects::WorkloadState {
                            agent_name: "fake_agent_1".to_string(),
                            workload_name: "fake_workload_1".to_string(),
                            execution_state: common::objects::ExecutionState::ExecUnknown,
                        },
                        common::objects::WorkloadState {
                            agent_name: "fake_agent_1".to_string(),
                            workload_name: "fake_workload_2".to_string(),
                            execution_state: common::objects::ExecutionState::ExecUnknown,
                        },
                    ],
                );
            }
            cmd => panic!("Unexpected command {:?}", cmd),
        }

        // Make sure that the queue is empty - we have read all messages.
        let agent_message = agents_receiver.try_recv();
        assert!(agent_message.is_err());
    }

    #[tokio::test]
    async fn utest_start_calls_agents_in_update_state_command() {
        let _ = env_logger::builder().is_test(true).try_init();

        const BUFFER_SIZE: usize = 20;
        let fake_agent_names = ["fake_agent_1", "fake_agent_2"];

        let (to_agents, mut agents_receiver) = mpsc::channel::<FromServer>(BUFFER_SIZE);
        let (to_server, server_receiver) = mpsc::channel::<ToServer>(BUFFER_SIZE);

        let mut server = AnkaiosServer::new(server_receiver, to_agents);

        // prepare workload specs
        let mut wl = HashMap::new();
        wl.insert(
            "fake_workload_spec_1".to_owned(),
            generate_test_workload_spec_with_param(
                fake_agent_names[0].to_owned(),
                "fake_workload_1".to_owned(),
                RUNTIME_NAME.to_string(),
            ),
        );
        wl.insert(
            "fake_workload_spec_2".to_owned(),
            generate_test_workload_spec_with_param(
                fake_agent_names[0].to_owned(),
                "fake_workload_2".to_owned(),
                RUNTIME_NAME.to_string(),
            ),
        );
        wl.insert(
            "fake_workload_spec_3".to_owned(),
            generate_test_workload_spec_with_param(
                fake_agent_names[1].to_owned(),
                "fake_workload_3".to_owned(),
                RUNTIME_NAME.to_string(),
            ),
        );

        // prepare current state
        server.current_complete_state = CompleteState {
            current_state: State {
                workloads: wl,
                configs: HashMap::default(),
                cron_jobs: HashMap::default(),
            },
            ..Default::default()
        };

        // prepare the new state (one workload to be updated, one workload to be removed)
        let mut new_state = server.current_complete_state.clone();
        new_state
            .current_state
            .workloads
            .get_mut("fake_workload_spec_1")
            .unwrap()
            .restart = false;
        new_state
            .current_state
            .workloads
            .remove("fake_workload_spec_2");

        let new_state_clone = new_state.clone();

        let agent_hello1_result = to_server.agent_hello(fake_agent_names[0].to_owned()).await;
        assert!(agent_hello1_result.is_ok());

        let agent_hello2_result = to_server.agent_hello(fake_agent_names[1].to_owned()).await;
        assert!(agent_hello2_result.is_ok());

        // prepare update state mock
        UPDATE_STATE_MOCK_RESULTS.with(move |results| {
            let mut results = results.borrow_mut();
            results.clear();
            results.push_back(Ok(new_state_clone));
        });
        UPDATE_STATE_MOCK_CALLS.with(move |calls| {
            let mut calls = calls.borrow_mut();
            calls.clear();
        });

        let update_mask = format!("workloads.{}", "fake_workload_spec_1");

        let update_state_result = to_server
            .update_state("requeste_id".into(), new_state, vec![update_mask.clone()])
            .await;
        assert!(update_state_result.is_ok());

        let handle = server.start();

        // The receiver in the server receives the messages and terminates the infinite waiting-loop
        drop(to_server);
        join!(handle);

        // UpdateWorkload triggered by the "Agent Hello" from the Fake Agent 1
        let agent_message = agents_receiver.try_recv();
        assert!(agent_message.is_ok());
        match agent_message.unwrap() {
            FromServer::UpdateWorkload(wl) => {
                assert_eq!(wl.added_workloads.len(), 2);
                assert_eq!(wl.deleted_workloads.len(), 0);
                check_update_workload(
                    Some(wl),
                    "fake_agent_1".to_string(),
                    vec!["fake_workload_1".to_string(), "fake_workload_2".to_string()],
                );
            }
            cmd => panic!("Unexpected command {:?}", cmd),
        }

        // UpdateWorkload triggered by the "Agent Hello" from the Fake Agent 2
        let agent_message = agents_receiver.try_recv();
        assert!(agent_message.is_ok());
        match agent_message.unwrap() {
            FromServer::UpdateWorkload(wl) => {
                assert_eq!(wl.added_workloads.len(), 1);
                assert_eq!(wl.deleted_workloads.len(), 0);
                check_update_workload(
                    Some(wl),
                    "fake_agent_2".to_string(),
                    vec!["fake_workload_3".to_string()],
                );
            }
            cmd => panic!("Unexpected command {:?}", cmd),
        }

        // UpdateWorkload triggered by the "Update State"
        let agent_message = agents_receiver.try_recv();
        assert!(agent_message.is_ok());
        match agent_message.unwrap() {
            FromServer::UpdateWorkload(wl) => {
                assert_eq!(wl.added_workloads.len(), 1);
                assert_eq!(wl.deleted_workloads.len(), 2);
                // TODO: this check shall be part of the "check_update_workload" function
                // I do not want to change this function, because we have two another feature branches which made changes here.
                // It would be difficult to merge them if we make changes in the function "check_update_workload"

                let mut wld = wl.deleted_workloads.clone();
                wld.sort_by(|a, b| a.name.cmp(&b.name));
                assert_eq!(
                    wld,
                    vec![
                        DeletedWorkload {
                            agent: "fake_agent_1".to_string(),
                            name: "fake_workload_spec_1".to_string(),
                            dependencies: HashMap::new(),
                        },
                        DeletedWorkload {
                            agent: "fake_agent_1".to_string(),
                            name: "fake_workload_spec_2".to_string(),
                            dependencies: HashMap::new(),
                        }
                    ]
                );
                check_update_workload(
                    Some(wl),
                    "fake_agent_1".to_string(),
                    vec!["fake_workload_1".to_string()],
                );
            }
            cmd => panic!("Unexpected command {:?}", cmd),
        }

        // Make sure that the queue is empty - we have read all messages.
        let agent_message = agents_receiver.try_recv();
        assert!(agent_message.is_err());
    }
}
