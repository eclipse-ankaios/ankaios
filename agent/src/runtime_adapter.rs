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

use async_trait::async_trait;
use common::objects::WorkloadSpec;

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait RuntimeAdapter {
    async fn start(&mut self, agent_name: &str, initial_workload_list: Vec<WorkloadSpec>);
    fn get_name(&self) -> &'static str;
    fn add_workload(&mut self, workload: WorkloadSpec);
    async fn update_workload(&mut self, workload: WorkloadSpec);
    async fn delete_workload(&mut self, workload_name: &str);
    async fn stop(&self);
}
