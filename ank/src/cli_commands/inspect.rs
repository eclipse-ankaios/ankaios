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

use ankaios_api::ank_base::UpdateStateRequest;
use prost::Message;
use std::fs;
use std::path::Path;

pub struct InspectCommand {
    input_path: String,
}

impl InspectCommand {
    pub fn new(input_path: String) -> Self {
        Self { input_path }
    }

    pub async fn execute(&self) -> Result<(), String> {
        let input_path = Path::new(&self.input_path);

        // Read binary protobuf
        let bytes = fs::read(input_path)
            .map_err(|e| format!("Failed to read file '{}': {}", self.input_path, e))?;

        let request = UpdateStateRequest::decode(&bytes[..])
            .map_err(|e| format!("Failed to decode protobuf: {}", e))?;

        // Extract State and metadata
        let state = request
            .new_state
            .and_then(|cs| cs.desired_state)
            .ok_or("No state found in request")?;

        // Print as YAML for human readability
        println!("=== Workload Manifest (unsigned content) ===");
        let state_yaml = serde_yaml::to_string(&state)
            .map_err(|e| format!("Failed to serialize state to YAML: {}", e))?;
        println!("{}", state_yaml);

        // Print signature metadata if present
        if let Some(metadata) = request.signature_metadata {
            println!("=== Signature Metadata ===");
            println!("Key ID:    {}", metadata.key_id);
            println!("Counter:   {}", metadata.counter);
            println!("Timestamp: {} ({})", metadata.timestamp, format_timestamp(metadata.timestamp));
            println!("Signature: {} bytes", metadata.signature.len());
        } else {
            println!("=== No Signature ===");
            println!("This manifest is not signed.");
        }

        Ok(())
    }
}

fn format_timestamp(unix_timestamp: u64) -> String {
    use chrono::{DateTime, Utc};
    use std::time::{Duration, UNIX_EPOCH};

    let duration = Duration::from_secs(unix_timestamp);
    let datetime = DateTime::<Utc>::from(UNIX_EPOCH + duration);
    datetime.format("%Y-%m-%d %H:%M:%S UTC").to_string()
}
