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
use common::objects::{AgentName, WorkloadState};
use common::to_server_interface::ToServer;
use generic_polling_state_checker::GenericPollingStateChecker;
use grpc::security::TLSConfig;
use std::collections::HashMap;
use tokio::try_join;

mod agent_manager;
mod cli;
mod control_interface;
mod runtime_connectors;
#[cfg(test)]
pub mod test_helper;
mod workload_operation;

mod generic_polling_state_checker;
mod runtime_manager;
mod workload;
mod workload_scheduler;
mod workload_state;

use common::from_server_interface::FromServer;
use common::std_extensions::{GracefulExitResult, IllegalStateResult, UnreachableResult};
use grpc::client::GRPCCommunicationsClient;

use agent_manager::AgentManager;

#[cfg_attr(test, mockall_double::double)]
use crate::runtime_manager::RuntimeManager;
use runtime_connectors::{
    podman::{PodmanRuntime, PodmanWorkloadId},
    podman_kube::{PodmanKubeRuntime, PodmanKubeWorkloadId},
    GenericRuntimeFacade, RuntimeConnector, RuntimeFacade,
};

const BUFFER_SIZE: usize = 20;

#[tokio::main]
async fn main() {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let args = cli::parse();
    let server_url = match args.insecure {
        true => args.server_url.replace("http[s]", "http"),
        false => args.server_url.replace("http[s]", "https"),
    };

    log::debug!(
        "Starting the Ankaios agent with \n\tname: '{}', \n\tserver url: '{}', \n\trun directory: '{}'",
        args.agent_name,
        server_url,
        args.run_folder,
    );

    // [impl->swdd~agent-uses-async-channels~1]
    let (to_manager, manager_receiver) = tokio::sync::mpsc::channel::<FromServer>(BUFFER_SIZE);
    let (to_server, server_receiver) = tokio::sync::mpsc::channel::<ToServer>(BUFFER_SIZE);
    let (workload_state_sender, workload_state_receiver) =
        tokio::sync::mpsc::channel::<WorkloadState>(BUFFER_SIZE);

    let run_directory = args
        .get_run_directory()
        .unwrap_or_exit("Run folder creation failed. Cannot continue without run folder.");

    // [impl->swdd~agent-supports-podman~2]
    let podman_runtime = Box::new(PodmanRuntime {});
    let podman_runtime_name = podman_runtime.name();
    let podman_facade = Box::new(GenericRuntimeFacade::<
        PodmanWorkloadId,
        GenericPollingStateChecker,
    >::new(podman_runtime));
    let mut runtime_facade_map: HashMap<String, Box<dyn RuntimeFacade>> = HashMap::new();
    runtime_facade_map.insert(podman_runtime_name, podman_facade);

    // [impl->swdd~agent-supports-podman-kube-runtime~1]
    let podman_kube_runtime = Box::new(PodmanKubeRuntime {});
    let podman_kube_runtime_name = podman_kube_runtime.name();
    let podman_kube_facade = Box::new(GenericRuntimeFacade::<
        PodmanKubeWorkloadId,
        GenericPollingStateChecker,
    >::new(podman_kube_runtime));
    runtime_facade_map.insert(podman_kube_runtime_name, podman_kube_facade);

    // The RuntimeManager currently directly gets the server ToServerInterface, but it shall get the agent manager interface
    // This is needed to be able to filter/authorize the commands towards the Ankaios server
    // The pipe connecting the workload to Ankaios must be in the runtime adapter
    let runtime_manager = RuntimeManager::new(
        AgentName::from(args.agent_name.as_str()),
        run_directory.get_path(),
        to_server.clone(),
        runtime_facade_map,
        workload_state_sender,
    );

    if let Err(err_message) = TLSConfig::is_config_conflicting(args.insecure, &args.ca_pem, &args.crt_pem, &args.key_pem) {
        log::warn!("{}", err_message);
    }

    // [impl->swdd~agent-establishes-insecure-communication-based-on-provided-insecure-cli-argument~1]
    // [impl->swdd~agent-provides-file-paths-to-communication-middleware~1]
    // [impl->swdd~agent-fails-on-missing-file-paths-and-insecure-cli-arguments~1]
    let tls_config = TLSConfig::new(args.insecure, args.ca_pem, args.crt_pem, args.key_pem);

    let communications_client = GRPCCommunicationsClient::new_agent_communication(
        args.agent_name.clone(),
        server_url,
        // [impl->swdd~agent-fails-on-missing-file-paths-and-insecure-cli-arguments~1]
        tls_config.unwrap_or_exit("Missing certificate file"),
    );

    let mut agent_manager = AgentManager::new(
        args.agent_name,
        manager_receiver,
        runtime_manager,
        to_server,
        workload_state_receiver,
    );

    let manager_task = tokio::spawn(async move { agent_manager.start().await });
    // [impl->swdd~agent-sends-hello~1]
    // [impl->swdd~agent-default-communication-grpc~1]
    let communications_task = tokio::spawn(async move {
        communications_client?
            .run(server_receiver, to_manager.clone())
            .await
    });

    let (_, communication_task_result) =
        try_join!(manager_task, communications_task).unwrap_or_illegal_state();

    communication_task_result.unwrap_or_unreachable();
}
