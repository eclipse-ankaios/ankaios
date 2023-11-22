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

use std::{collections::HashMap, io, path::Path, time::Duration, vec, process::exit};
use api::proto;

use prost::Message;
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncWriteExt},
};

const MAX_VARINT_SIZE: usize = 19;
const WAITING_TIME_IN_SEC: usize = 5;

fn log(msg: &str) {
    println!("[{}] {}", chrono::offset::Utc::now().format("%Y-%m-%dT%H:%M:%SZ"), msg);
}

#[tokio::main]
async fn main() {
    let pipes_location = Path::new("/run/ankaios/control_interface");
    std::fs::create_dir_all(pipes_location).unwrap();

    let ex_req_fifo = pipes_location.join("input");
    let sc_req_fifo = pipes_location.join("output");

    let mut ex_req = File::open(&ex_req_fifo).await.unwrap_or_else(|err| {
        log(&format!("Error: cannot open '{}': '{}'", ex_req_fifo.to_str().unwrap(), err));
        exit(1);
    });
    let mut sc_req = File::create(&sc_req_fifo).await.unwrap_or_else(|err| {
        log(&format!("Error: cannot create '{}': '{}'", sc_req_fifo.to_str().unwrap(), err));
        exit(1);
    });

    
    tokio::spawn(async move {
        loop { 
            if let Ok(binary) = read_protobuf_data(&mut ex_req).await {
                let proto = proto::ExecutionRequest::decode(&mut Box::new(binary.as_ref()));

                log(&format!("Receiving ExecutionRequest containing the workload states of the current state: {:?}", proto));
            }
        }
    });

    tokio::time::sleep(Duration::from_secs(WAITING_TIME_IN_SEC)).await;

    let mut wl = HashMap::new();
    let wl_api = proto::Workload {
        runtime: "podman".to_string(),
        agent: "agent_A".to_string(),
        restart: false,
        update_strategy: proto::UpdateStrategy::AtMostOnce.into(),
        access_rights: None,
        tags: vec![proto::Tag {
            key: "owner".to_string(),
            value: "Ankaios team".to_string(),
        }],
        runtime_config: "image: docker.io/library/nginx\ncommandOptions: [\"-p\", \"8080:80\"]".to_string(),
        dependencies: HashMap::new(),
    };

    wl.insert("dynamic_nginx".to_string(), wl_api);

    let proto_buf_update = proto::StateChangeRequest {
        state_change_request_enum: Some(
            proto::state_change_request::StateChangeRequestEnum::UpdateState(
                proto::UpdateStateRequest {
                    new_state: Some(proto::CompleteState {
                        current_state: Some(proto::State {
                            workloads: wl,
                            configs: HashMap::new(),
                            cronjobs: HashMap::new(),
                        }),
                        ..Default::default()
                    }),
                    update_mask: vec![
                        "currentState.workloads.dynamic_nginx".to_string()
                    ],
                },
            ),
        ),
    };

    log(format!("Sending StateChangeRequest containing details for adding the dynamic workload \"dynamic_nginx\": {:?}", proto_buf_update).as_str());

    sc_req
        .write_all(&proto_buf_update.encode_length_delimited_to_vec())
        .await
        .unwrap();

    let mut i = 0;

    loop {
        let protobuf = proto::StateChangeRequest {
            state_change_request_enum: Some(
                proto::state_change_request::StateChangeRequestEnum::RequestCompleteState(
                    proto::RequestCompleteState {
                        request_id: i.to_string(),

                        field_mask: vec![String::from("workloadStates")],
                    },
                ),
            ),
        };

        i += 1;

        log(format!("Sending StateChangeRequest containing details for requesting all workload states: {:?}", protobuf).as_str());
        sc_req
            .write_all(&protobuf.encode_length_delimited_to_vec())
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_secs(WAITING_TIME_IN_SEC)).await;
    }
}

async fn read_protobuf_data(file: &mut File) -> Result<Box<[u8]>, io::Error> {
    let varint_data = read_varint_data(file).await?;
    let mut varint_data = Box::new(&varint_data[..]);

    // determine the exact size for exact reading of the bytes later by decoding the varint data
    let size = prost::encoding::decode_varint(&mut varint_data)? as usize;

    let mut buf = vec![0; size];
    file.read_exact(&mut buf[..]).await?; // read exact bytes from file
    Ok(buf.into_boxed_slice())
}

async fn read_varint_data(file: &mut File) -> Result<[u8; MAX_VARINT_SIZE], io::Error> {
    let mut res = [0u8; MAX_VARINT_SIZE];
    for item in res.iter_mut() {
        *item = file.read_u8().await?;
        // check if most significant bit is set to 0 if so it is the last byte to be read
        if *item & 0b10000000 == 0 {
            break;
        }
    }
    Ok(res)
}
