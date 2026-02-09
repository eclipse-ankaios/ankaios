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

use ankaios_sdk::{Ankaios, LogResponse, LogsRequest};

#[tokio::main]
async fn main() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .init();

    // Create a new Ankaios object.
    // The connection to the control interface is automatically done at this step.
    let mut ank = Ankaios::new().await.expect("Failed to initialize");

    // Get the workload instance names of the workloads running on the system
    let mut screamer_instance_name = None;
    if let Ok(complete_state) = ank.get_state(vec!["workloadStates".to_owned()]).await {
        // Get the workload states present in the complete state
        let workload_states = Vec::from(complete_state.get_workload_states());

        // Get the workload instance name of the "screamer" workload
        for workload_state in workload_states {
            if workload_state.workload_instance_name.workload_name == "screamer" {
                screamer_instance_name = Some(workload_state.workload_instance_name.clone());
                break;
            }
        }
    }

    if screamer_instance_name.is_none() {
        println!("No 'screamer' workload found. Please start the screamer workload first.");
        std::process::exit(1);
    }
    let screamer_instance_name = screamer_instance_name.unwrap();

    let logs_request = LogsRequest {
        workload_names: vec![screamer_instance_name.clone()],
        follow: true,
        ..Default::default()
    };

    // Request the logs from the new workload
    let mut log_campaign_response = ank
        .request_logs(logs_request)
        .await
        .expect("Failed to request logs");

    // Check if the workload was accepted for log retrieval
    if !log_campaign_response
        .accepted_workload_names
        .contains(&screamer_instance_name)
    {
        println!(
            "Workload '{}' not accepted for log retrieval",
            screamer_instance_name
        );

        std::process::exit(1);
    }

    // Listen for log responses until stop
    while let Some(log_response) = log_campaign_response.logs_receiver.recv().await {
        match log_response {
            LogResponse::LogEntries(log_entries) => {
                for entry in log_entries {
                    println!("Log from {}: {}", entry.workload_name, entry.message);
                }
            }
            LogResponse::LogsStopResponse(workload_name) => {
                println!(
                    "No more logs available for workload '{}'. Stopping log retrieval.",
                    workload_name
                );
                break;
            }
        }
    }

    // Stop receiving logs for the workload
    ank.stop_receiving_logs(log_campaign_response)
        .await
        .expect("Failed to stop receiving logs");
    println!(
        "Stopped log retrieval for workload '{}'.",
        screamer_instance_name
    );
}
