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

use ank_agent::runtime_connectors::{
    containerd::ContainerdRuntime, podman::PodmanRuntime, podman_kube::PodmanKubeRuntime,
};
use ank_agent::{AgentBuilder, AgentConfig};

#[tokio::main]
async fn main() {
    let agent_config = AgentConfig::load();

    AgentBuilder::new()
        // [impl->swdd~agent-supports-podman~2]
        .with_runtime(Box::new(PodmanRuntime {}))
        // [impl->swdd~agent-supports-podman-kube-runtime~1]
        .with_runtime(Box::new(PodmanKubeRuntime::default()))
        // [impl->swdd~agent-supports-containerd~1]
        .with_runtime(Box::new(ContainerdRuntime {}))
        .run(agent_config)
        .await;
}
