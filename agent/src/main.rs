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

use common::communications_client::CommunicationsClient;
use common::state_change_interface::StateChangeCommand;
use generic_polling_state_checker::GenericPollingStateChecker;
use std::collections::HashMap;
use tokio::try_join;

mod agent_manager;
mod cli;
mod control_interface;
mod parameter_storage;
mod podman;
#[cfg(test)]
pub mod test_helper;
mod workload_facade;
mod workload_trait;

mod generic_polling_state_checker;
mod runtime;
mod runtime_manager;
mod stoppable_state_checker;
mod workload;
mod runtime_facade;

use runtime_facade::GenericRuntimeFacade;

use common::execution_interface::ExecutionCommand;
use common::std_extensions::{GracefulExitResult, IllegalStateResult, UnreachableResult};
use grpc::client::GRPCCommunicationsClient;

use agent_manager::AgentManager;

use podman::{PodmanKubeRuntime, PodmanKubeWorkloadId};

use crate::runtime::Runtime;
use crate::runtime_manager::RuntimeManager;
use crate::runtime_facade::RuntimeFacade;

const BUFFER_SIZE: usize = 20;

#[tokio::main]
async fn main() {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let args = cli::parse();

    log::info!(
        "Starting the Ankaios agent with \n\tname: {}, \n\tserver url: {}, \n\tpodman socket path: {}, \n\trun directory: {}",
        args.agent_name,
        args.server_url,
        args.podman_socket_path,
        args.run_folder,
    );

    // [impl->swdd~agent-uses-async-channels~1]
    let (to_manager, manager_receiver) =
        tokio::sync::mpsc::channel::<ExecutionCommand>(BUFFER_SIZE);
    let (to_server, server_receiver) =
        tokio::sync::mpsc::channel::<StateChangeCommand>(BUFFER_SIZE);

    let run_directory = args
        .get_run_directory()
        .unwrap_or_exit("Run folder creation failed. Cannot continue without run folder.");

    // [impl->swdd~agent-supports-podman~1]
    let podman_kube_runtime = Box::new(PodmanKubeRuntime {});
    let podman_kube_runtime_name = podman_kube_runtime.name();
    let podman_kube_facade = Box::new(GenericRuntimeFacade::<
        PodmanKubeWorkloadId,
        GenericPollingStateChecker,
    >::new(podman_kube_runtime));
    let mut runtime_facade_map: HashMap<String, Box<dyn RuntimeFacade>> = HashMap::new();
    runtime_facade_map.insert(podman_kube_runtime_name, podman_kube_facade);

    // The RuntimeManager currently directly gets the server StateChangeInterface, but it shall get the agent manager interface
    // This is needed to be able to filter/authorize the commands towards the Ankaios server
    // The pipe connecting the workload to Ankaios must be in the runtime adapter
    let runtime_manager = RuntimeManager::new(
        args.agent_name.clone().into(),
        run_directory.get_path(),
        runtime_facade_map,
    );

    let mut grpc_communications_client =
        GRPCCommunicationsClient::new_agent_communication(args.agent_name.clone(), args.server_url);

    let mut agent_manager = AgentManager::new(
        args.agent_name,
        manager_receiver,
        runtime_manager,
        to_server,
    );

    let manager_task = tokio::spawn(async move { agent_manager.start().await });
    // [impl->swdd~agent-sends-hello~1]
    // [impl->swdd~agent-default-communication-grpc~1]
    let communications_task = tokio::spawn(async move {
        grpc_communications_client
            .run(server_receiver, to_manager.clone())
            .await
    });

    let (_, communication_task_result) =
        try_join!(manager_task, communications_task).unwrap_or_illegal_state();

    communication_task_result.unwrap_or_unreachable();
}
