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

#[cfg(test)]
use tests::update_state_mock as update_state;
#[cfg(not(test))]
use update_state::update_state;

use common::commands::{CompleteState, RequestCompleteState};
use common::execution_interface::ExecutionCommand;
use common::objects::State;
use common::{execution_interface::ExecutionInterface, state_change_interface::StateChangeCommand};
use tokio::sync::mpsc::{channel, Receiver, Sender};

use crate::ankaios_server::update_state::prepare_update_workload;
use crate::state_manipulation::Object;
use crate::workload_state_db::WorkloadStateDB;

pub type StateChangeChannels = (Sender<StateChangeCommand>, Receiver<StateChangeCommand>);
pub type ExecutionChannels = (Sender<ExecutionCommand>, Receiver<ExecutionCommand>);

pub fn create_state_change_channels(capacity: usize) -> StateChangeChannels {
    channel::<StateChangeCommand>(capacity)
}
pub fn create_execution_channels(capacity: usize) -> ExecutionChannels {
    channel::<ExecutionCommand>(capacity)
}

pub struct AnkaiosServer {
    // [impl->swdd~server-uses-async-channels~1]
    receiver: Receiver<StateChangeCommand>,
    // [impl->swdd~communication-to-from-server-middleware~1]
    to_agents: Sender<ExecutionCommand>,
    current_complete_state: CompleteState,
    workload_state_db: WorkloadStateDB,
}

impl AnkaiosServer {
    pub fn new(
        receiver: Receiver<StateChangeCommand>,
        to_agents: Sender<ExecutionCommand>,
    ) -> Self {
        AnkaiosServer {
            receiver,
            to_agents,
            current_complete_state: CompleteState::default(),
            workload_state_db: WorkloadStateDB::default(),
        }
    }

    fn get_complete_state_by_field_mask(
        &self,
        request_complete_state: &RequestCompleteState,
    ) -> Option<CompleteState> {
        let current_complete_state = CompleteState {
            request_id: request_complete_state.request_id.to_owned(),
            current_state: self.current_complete_state.current_state.clone(),
            startup_state: self.current_complete_state.startup_state.clone(),
            workload_states: self.workload_state_db.get_all_workload_states(),
        };

        // [impl->swdd~server-filters-get-complete-state-result~1]
        if !request_complete_state.field_mask.is_empty() {
            let current_complete_state: Object = current_complete_state.try_into().unwrap();
            let mut return_state = Object::default();

            return_state
                .set(
                    &"requestId".into(),
                    request_complete_state.request_id.to_owned().into(),
                )
                .expect("unreachable");

            for field in &request_complete_state.field_mask {
                if let Some(value) = current_complete_state.get(&field.into()) {
                    if return_state.set(&field.into(), value.to_owned()).is_err() {
                        log::debug!(concat!(
                            "Result for CompleteState incomplete, as requested field could not be set:\n",
                            "   request_id: {:?}\n",
                            "   field: {}"),
                            request_complete_state.request_id, field);
                    };
                } else {
                    log::debug!(
                        concat!(
                        "Result for CompleteState incomplete, as requested field does not exist:\n",
                        "   request_id: {:?}\n",
                        "   field: {}"),
                        request_complete_state.request_id,
                        field
                    );
                    continue;
                };
            }

            match return_state.try_into() {
                Ok(return_state) => Some(return_state),
                Err(error) => {
                    log::error!(
                        "The result for CompleteState is invalid and could not be returned: '{error}'");
                    None
                }
            }
        } else {
            Some(current_complete_state)
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
                StateChangeCommand::AgentHello(method_obj) => {
                    log::debug!("Received AgentHello from communications server");

                    // Send this agent all workloads in the current state which are assigned to him
                    // [impl->swdd~server-sends-all-workloads-on-start~1]
                    self.to_agents
                        .update_workload(
                            self.current_complete_state
                                .current_state
                                .workloads
                                .clone()
                                .into_values()
                                // [impl->swdd~agent-from-agent-field~1]
                                .filter(|workload_spec| {
                                    workload_spec.agent.eq(&method_obj.agent_name)
                                })
                                .collect(),
                            // It's a newly connected agent, no need to delete anything.
                            vec![],
                        )
                        .await;

                    // [impl->swdd~server-informs-a-newly-connected-agent-workload-states~1]
                    // [impl->swdd~server-sends-all-workload-states-on-agent-connect~1]
                    let workload_states = self
                        .workload_state_db
                        .get_workload_state_excluding_agent(&method_obj.agent_name);
                    if !workload_states.is_empty() {
                        self.to_agents.update_workload_state(workload_states).await;
                    } else {
                        log::debug!("No workload states to send. Nothing to do.");
                    }
                }
                StateChangeCommand::AgentGone(method_obj) => {
                    log::debug!("Received AgentGone from communications server");
                    // [impl->swdd~server-set-workload-state-unknown-on-disconnect~1]
                    self.workload_state_db
                        .mark_all_workload_state_for_agent_unknown(&method_obj.agent_name);

                    // now tell everybody the exciting news ;)
                    // [impl->swdd~server-distribute-workload-state-unknown-on-disconnect~1]
                    self.to_agents
                        .update_workload_state(
                            self.workload_state_db
                                .get_workload_state_for_agent(&method_obj.agent_name),
                        )
                        .await;
                }
                // [impl->swdd~server-provides-update-current-state-interface~1]
                StateChangeCommand::UpdateState(update_request) => {
                    log::debug!("Received UpdateState from communications server");

                    match update_state(&self.current_complete_state, update_request) {
                        Ok(new_state) => {
                            let cmd = prepare_update_workload(
                                &self.current_complete_state.current_state,
                                &new_state.current_state,
                            );

                            if cmd.is_some() {
                                self.to_agents.send(cmd.unwrap()).await.unwrap();
                            } else {
                                log::debug!("The current state and new state are identical -> nothing to do");
                            }
                            self.current_complete_state = new_state;
                        }
                        Err(error) => {
                            log::warn!("Could not execute UpdateRequest: '{}'", error)
                        }
                    }
                }
                StateChangeCommand::UpdateWorkloadState(method_obj) => {
                    log::debug!(
                        "Received UpdateWorkloadState from communications server: {method_obj:?}"
                    );

                    // [impl->swdd~server-stores-workload-state~1]
                    self.workload_state_db
                        .insert(method_obj.workload_states.clone());

                    // [impl->swdd~server-forwards-workload-state~1]
                    self.to_agents
                        .update_workload_state(method_obj.workload_states)
                        .await;
                }
                // [impl->swdd~server-provides-interface-get-complete-state~1]
                // [impl->swdd~server-includes-id-in-control-interface-response~1]
                StateChangeCommand::RequestCompleteState(method_obj) => {
                    log::debug!(
                        "Received RequestCompleteState from communications server: {method_obj:?}"
                    );

                    if let Some(complete_state) = self.get_complete_state_by_field_mask(&method_obj)
                    {
                        self.to_agents.complete_state(complete_state).await;
                    } else {
                        self.to_agents
                            .complete_state(common::commands::CompleteState {
                                request_id: method_obj.request_id,
                                startup_state: State::default(),
                                current_state: State::default(),
                                workload_states: vec![],
                            })
                            .await
                    }
                }
                StateChangeCommand::Stop(_method_obj) => {
                    log::debug!("Received Stop from communications server");
                    // TODO: handle the call
                    // for operator in self.operator_map.values() {
                    //     operator.stop().await;
                    // }

                    break;
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

    use common::commands::{RequestCompleteState, UpdateStateRequest};
    use common::objects::{
        AccessRights, DeletedWorkload, RuntimeWorkload, State, Tag, WorkloadSpec, WorkloadState,
    };
    use common::{
        commands::CompleteState,
        execution_interface::ExecutionCommand,
        state_change_interface::{StateChangeCommand, StateChangeInterface},
    };
    use tokio::join;
    use tokio::sync::mpsc::{self, channel, Receiver, Sender};

    use super::update_state::Error;
    use super::{create_execution_channels, create_state_change_channels, AnkaiosServer};

    type TestSetup = (
        (
            AnkaiosServer,
            tokio::task::JoinHandle<()>,
            tokio::task::JoinHandle<()>,
            tokio::task::JoinHandle<()>,
        ), // ( server instance, communication mapper task, fake agent 1 task, fake agent 2 task)
        (Sender<StateChangeCommand>, Sender<ExecutionCommand>), // (state change sender channel to ankaios server, execution sender channel to communication mapper)
        Receiver<TestResult>,                                   // test result receiver channel
    );

    #[derive(PartialEq, Debug, Clone)]
    enum TestResult {
        Result(String),
    }

    #[derive(Debug)]
    struct CommunicationMapper {
        fake_agents: HashMap<String, Sender<ExecutionCommand>>,
        ex_receiver: Receiver<ExecutionCommand>,
    }

    impl CommunicationMapper {
        fn new(ex_receiver: Receiver<ExecutionCommand>) -> Self {
            CommunicationMapper {
                fake_agents: HashMap::new(),
                ex_receiver,
            }
        }
        fn insert(&mut self, agent_name: String, to_agent: Sender<ExecutionCommand>) {
            self.fake_agents.insert(agent_name, to_agent);
        }

        async fn start(&mut self) {
            while let Some(ex_command) = self.ex_receiver.recv().await {
                match ex_command {
                    ExecutionCommand::UpdateWorkload(update_workload) => {
                        let agent_names: Vec<String> = update_workload
                            .added_workloads
                            .iter()
                            .map(|wl| wl.agent.clone())
                            .collect();

                        let relevant_agents: Vec<(String, Sender<ExecutionCommand>)> = self
                            .fake_agents
                            .clone()
                            .into_iter()
                            .filter(|x| agent_names.iter().any(|y| y == &x.0))
                            .collect();

                        for (_, agent_sender) in relevant_agents.into_iter() {
                            agent_sender
                                .send(ExecutionCommand::UpdateWorkload(update_workload.clone()))
                                .await
                                .unwrap();
                        }
                    }
                    ExecutionCommand::UpdateWorkloadState(update_workload_state) => {
                        let agent_names: Vec<String> = update_workload_state
                            .workload_states
                            .iter()
                            .map(|wls| wls.agent_name.clone())
                            .collect();

                        let relevant_agents: Vec<(String, Sender<ExecutionCommand>)> = self
                            .fake_agents
                            .clone()
                            .into_iter()
                            .filter(|x| agent_names.iter().any(|y| y == &x.0))
                            .collect();

                        for (_, agent_sender) in relevant_agents.into_iter() {
                            agent_sender
                                .send(ExecutionCommand::UpdateWorkloadState(
                                    update_workload_state.clone(),
                                ))
                                .await
                                .unwrap();
                        }
                    }
                    ExecutionCommand::CompleteState(mut boxed_complete_state) => {
                        let mut splitted = boxed_complete_state.request_id.split('@');
                        let agent_name = splitted.next().unwrap();
                        let request_id = splitted.next().unwrap();
                        let agent_sender = self.fake_agents.get(agent_name).unwrap();
                        boxed_complete_state.request_id = request_id.to_owned();
                        agent_sender
                            .send(ExecutionCommand::CompleteState(boxed_complete_state))
                            .await
                            .unwrap();
                    }
                    _ => panic!(),
                }
            }
        }
    }

    struct FakeAgent {
        ex_receiver: Receiver<ExecutionCommand>,
        tc_sender: Sender<TestResult>,
    }

    impl FakeAgent {
        fn new(ex_receiver: Receiver<ExecutionCommand>, tc_sender: Sender<TestResult>) -> Self {
            FakeAgent {
                ex_receiver,
                tc_sender,
            }
        }

        async fn start<F, Fut>(&mut self, handler: F)
        where
            F: Fn(Sender<TestResult>, ExecutionCommand) -> Fut,
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
                ExecutionCommand::UpdateWorkload(update_workload) => {
                    tcs.send(TestResult::Result(
                        serde_json::to_string(&update_workload).unwrap().to_owned(),
                    ))
                    .await
                    .unwrap();
                }
                ExecutionCommand::UpdateWorkloadState(update_workload_state) => tcs
                    .send(TestResult::Result(
                        serde_json::to_string(&update_workload_state)
                            .unwrap()
                            .to_owned(),
                    ))
                    .await
                    .unwrap(),

                ExecutionCommand::CompleteState(boxed_complete_state) => tcs
                    .send(TestResult::Result(
                        serde_json::to_string(&boxed_complete_state)
                            .unwrap()
                            .to_owned(),
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

    fn create_fake_workload_spec(agent_name: String, workload_name: String) -> WorkloadSpec {
        WorkloadSpec {
            agent: agent_name,
            runtime: "fake_runtime".to_owned(),
            dependencies: HashMap::default(),
            access_rights: AccessRights::default(),
            update_strategy: common::objects::UpdateStrategy::Unspecified,
            workload: common::objects::RuntimeWorkload {
                name: workload_name,
                restart: false,
                tags: vec![
                    Tag {
                        key: "tag_key_1".into(),
                        value: "tag_value_1".into(),
                    },
                    Tag {
                        key: "tag_key_2".into(),
                        value: "tag_value_2".into(),
                    },
                ],
                runtime_config: "fake_runtime_config".to_owned(),
            },
        }
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

    fn get_complete_state(
        result_from_fake_agent: &TestResult,
    ) -> Option<common::commands::CompleteState> {
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
                .map(|wls| (wls.agent, wls.workload.name))
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
    ) -> Result<CompleteState, Error> {
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
        static UPDATE_STATE_MOCK_RESULTS: RefCell<VecDeque<Result<CompleteState, Error>>> = RefCell::new(VecDeque::new());
    }

    // [utest->swdd~server-sends-all-workloads-on-start~1]
    // [utest->swdd~agent-from-agent-field~1]
    #[tokio::test]
    async fn utest_server_sends_workloads_and_workload_states() {
        // prepare test setup
        let fake_agent_names = vec!["fake_agent_1", "fake_agent_2"];
        let (
            (mut ankaios_server, cm_server_task, fake_agent_1_task, fake_agent_2_task),
            (to_server, _to_cm_server),
            mut tc_receiver,
        ) = create_test_setup();

        // prepare workload specs
        let mut wl = HashMap::new();
        wl.insert(
            "fake_workload_spec_1".to_owned(),
            create_fake_workload_spec(fake_agent_names[0].to_owned(), "fake_workload_1".to_owned()),
        );
        wl.insert(
            "fake_workload_spec_2".to_owned(),
            create_fake_workload_spec(fake_agent_names[0].to_owned(), "fake_workload_2".to_owned()),
        );
        wl.insert(
            "fake_workload_spec_3".to_owned(),
            create_fake_workload_spec(fake_agent_names[1].to_owned(), "fake_workload_3".to_owned()),
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
        to_server.agent_hello(fake_agent_names[0].to_owned()).await;

        check_update_workload(
            get_workloads(&tc_receiver.recv().await.unwrap()),
            "fake_agent_1".to_string(),
            vec!["fake_workload_1".to_string(), "fake_workload_2".to_string()],
        );

        // [utest->swdd~server-informs-a-newly-connected-agent-workload-states~1]
        // [utest->swdd~server-sends-all-workload-states-on-agent-connect~1]
        // send update_workload_state for fake_agent_1 which is then stored in the workload_state_db in ankaios server
        to_server
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
        to_server.agent_hello(fake_agent_names[1].to_owned()).await;

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
        to_server
            .update_workload_state(vec![common::objects::WorkloadState {
                agent_name: fake_agent_names[1].to_string(),
                workload_name: "fake_workload_3".to_string(),
                execution_state: common::objects::ExecutionState::ExecSucceeded,
            }])
            .await;

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
        to_server
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
        let agent_names = vec!["fake_agent_1", "fake_agent_2"];
        let workload_names = vec!["workload_1", "workload_2"];
        let request_id = "id1";
        let update_mask = format!("workloads.{}", workload_names[1]);

        // prepare structures
        let workloads = vec![
            create_fake_workload_spec(agent_names[0].to_owned(), workload_names[0].to_owned()),
            create_fake_workload_spec(agent_names[1].to_owned(), workload_names[1].to_owned()),
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

        to_server.agent_hello(agent_names[0].to_owned()).await;

        // send new state to server
        to_server
            .update_state(update_state.clone(), vec![update_mask.clone()])
            .await;

        // request complete state
        to_server
            .request_complete_state(RequestCompleteState {
                request_id: format!("{}@{}", agent_names[0], request_id),
                field_mask: vec![],
            })
            .await;

        let _ignore_added_workloads = tc_receiver.recv().await;
        let complete_state = tc_receiver.recv().await;

        let expected_complete_state = CompleteState {
            request_id: request_id.into(),
            startup_state: State {
                workloads: HashMap::new(),
                configs: HashMap::new(),
                cron_jobs: HashMap::new(),
            },
            current_state: expected_state.current_state.clone(),
            workload_states: vec![],
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
            create_fake_workload_spec(
                agent_name_fake_agent_1.to_owned(),
                "fake_workload_1".to_owned(),
            ),
        );
        workloads.insert(
            "fake_workload_spec_2".to_owned(),
            create_fake_workload_spec(
                agent_name_fake_agent_1.to_owned(),
                "fake_workload_2".to_owned(),
            ),
        );
        workloads.insert(
            "fake_workload_spec_3".to_owned(),
            create_fake_workload_spec(
                agent_name_fake_agent_1.to_owned(),
                "fake_workload_3".to_owned(),
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

        let check_workload_state =
            |next_result: TestResult, expected_complete_state: &CompleteState| {
                if let Some(complete_state) = get_complete_state(&next_result) {
                    assert_eq!(
                        expected_complete_state.request_id,
                        complete_state.request_id
                    );
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

        to_server
            .agent_hello(agent_name_fake_agent_1.to_owned())
            .await;

        let _skip_hello_result_as_not_on_test_focus = tc_receiver.recv().await.unwrap();

        // send command 'RequestCompleteState' with empty field mask meaning without active filter
        // so CompleteState shall contain the complete state
        to_server
            .request_complete_state(super::RequestCompleteState {
                request_id: format!("{agent_name_fake_agent_1}@my_request_id"),
                field_mask: vec![],
            })
            .await;

        check_workload_state(
            tc_receiver.recv().await.unwrap(),
            &CompleteState {
                request_id: String::from("my_request_id"),
                startup_state: State::default(),
                workload_states: vec![],
                current_state: test_state.current_state.clone(),
            },
        );

        // send command 'RequestCompleteState' with field mask = ["workloadStates"]
        to_server
            .request_complete_state(super::RequestCompleteState {
                request_id: format!("{agent_name_fake_agent_1}@my_request_id"),
                field_mask: vec![
                    String::from("workloadStates"),
                    String::from("currentState.workloads.fake_workload_spec_1"),
                    String::from("currentState.workloads.fake_workload_spec_3.tags"),
                    String::from("currentState.workloads.fake_workload_spec_4"),
                ],
            })
            .await;

        check_workload_state(
            tc_receiver.recv().await.unwrap(),
            &CompleteState {
                request_id: String::from("my_request_id"),
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
                                workload: RuntimeWorkload {
                                    tags: vec![
                                        Tag {
                                            key: "tag_key_1".into(),
                                            value: "tag_value_1".into(),
                                        },
                                        Tag {
                                            key: "tag_key_2".into(),
                                            value: "tag_value_2".into(),
                                        },
                                    ],
                                    ..Default::default()
                                },
                                ..Default::default()
                            },
                        ),
                    ]
                    .into_iter()
                    .collect(),
                    ..Default::default()
                },
                ..Default::default()
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
        let fake_agent_names = vec!["fake_agent_1", "fake_agent_2"];

        let (to_agents, mut agents_receiver) = mpsc::channel::<ExecutionCommand>(BUFFER_SIZE);
        let (to_server, server_receiver) = mpsc::channel::<StateChangeCommand>(BUFFER_SIZE);

        let mut server = AnkaiosServer::new(server_receiver, to_agents);

        // prepare workload specs
        let mut wl = HashMap::new();
        wl.insert(
            "fake_workload_spec_1".to_owned(),
            create_fake_workload_spec(fake_agent_names[0].to_owned(), "fake_workload_1".to_owned()),
        );
        wl.insert(
            "fake_workload_spec_2".to_owned(),
            create_fake_workload_spec(fake_agent_names[0].to_owned(), "fake_workload_2".to_owned()),
        );
        wl.insert(
            "fake_workload_spec_3".to_owned(),
            create_fake_workload_spec(fake_agent_names[1].to_owned(), "fake_workload_3".to_owned()),
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

        to_server.agent_hello(fake_agent_names[0].to_owned()).await;
        to_server.agent_hello(fake_agent_names[1].to_owned()).await;

        // send update_workload_state for fake_agent_1 which is then stored in the workload_state_db in ankaios server
        to_server
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

        // fake_agent_1 disconnects from the ankaios server
        to_server.agent_gone(fake_agent_names[0].to_owned()).await;

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
            ExecutionCommand::UpdateWorkload(wl) => {
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
            ExecutionCommand::UpdateWorkload(wl) => {
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
            ExecutionCommand::UpdateWorkloadState(wls) => {
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
            ExecutionCommand::UpdateWorkloadState(wls) => {
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
        let fake_agent_names = vec!["fake_agent_1", "fake_agent_2"];

        let (to_agents, mut agents_receiver) = mpsc::channel::<ExecutionCommand>(BUFFER_SIZE);
        let (to_server, server_receiver) = mpsc::channel::<StateChangeCommand>(BUFFER_SIZE);

        let mut server = AnkaiosServer::new(server_receiver, to_agents);

        // prepare workload specs
        let mut wl = HashMap::new();
        wl.insert(
            "fake_workload_spec_1".to_owned(),
            create_fake_workload_spec(fake_agent_names[0].to_owned(), "fake_workload_1".to_owned()),
        );
        wl.insert(
            "fake_workload_spec_2".to_owned(),
            create_fake_workload_spec(fake_agent_names[0].to_owned(), "fake_workload_2".to_owned()),
        );
        wl.insert(
            "fake_workload_spec_3".to_owned(),
            create_fake_workload_spec(fake_agent_names[1].to_owned(), "fake_workload_3".to_owned()),
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
            .workload
            .restart = true;
        new_state
            .current_state
            .workloads
            .remove("fake_workload_spec_2");

        let new_state_clone = new_state.clone();

        to_server.agent_hello(fake_agent_names[0].to_owned()).await;
        to_server.agent_hello(fake_agent_names[1].to_owned()).await;

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

        to_server
            .update_state(new_state, vec![update_mask.clone()])
            .await;

        let handle = server.start();

        // The receiver in the server receives the messages and terminates the infinite waiting-loop
        drop(to_server);
        join!(handle);

        // UpdateWorkload triggered by the "Agent Hello" from the Fake Agent 1
        let agent_message = agents_receiver.try_recv();
        assert!(agent_message.is_ok());
        match agent_message.unwrap() {
            ExecutionCommand::UpdateWorkload(wl) => {
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
            ExecutionCommand::UpdateWorkload(wl) => {
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
            ExecutionCommand::UpdateWorkload(wl) => {
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
