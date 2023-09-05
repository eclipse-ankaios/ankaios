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

use std::collections::HashMap;

use serde::Serialize;

#[derive(Serialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct ServerListContainer {
    pub auto_remove: bool,
    pub command: Vec<String>,
    pub created: String,
    pub created_at: String,
    pub exit_code: i32,
    pub exited: bool,
    pub exited_at: i64,
    pub id: Option<String>,
    pub image: String,
    pub image_i_d: String,
    pub is_infra: bool,
    pub labels: HashMap<String, String>,
    pub mounts: Vec<String>,
    pub names: Vec<String>,
    pub namespaces: ServerListContainerNamespaces,
    pub networks: Vec<String>,
    pub pid: i64,
    pub pod: String,
    pub pod_name: String,
    pub ports: Vec<ServerListContainerPort>,
    pub size: ServerListContainerSize,
    pub started_at: i64,
    pub state: String,
    pub status: String,
}

impl ServerListContainer {
    pub fn new() -> ServerListContainer {
        ServerListContainer {
            created: String::from("2019-08-24T14:15:22Z"),
            ..Default::default()
        }
    }
}

#[derive(Serialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct ServerListContainerNamespaces {
    pub cgroup: String,
    pub ipc: String,
    pub mnt: String,
    pub net: String,
    pub pidns: String,
    pub user: String,
    pub uts: String,
}

#[derive(Serialize)]
pub struct ServerListContainerPort {
    pub container_port: u16,
    pub host_ip: String,
    pub host_port: u16,
    pub protocol: String,
    pub range: u16,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ServerListContainerSize {
    pub root_fs_size: i64,
    pub rw_size: i64,
}

#[derive(Serialize)]
pub struct ServerError {
    pub cause: String,
    pub message: String,
    pub response: i64,
}

#[derive(Serialize)]
pub struct ServerPullImages {
    pub id: String,
    pub images: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ServerContainerCreate {
    pub id: String,
    pub warnings: Vec<String>,
}
