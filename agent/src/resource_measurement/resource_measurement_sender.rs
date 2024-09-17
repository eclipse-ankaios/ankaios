// Copyright (c) 2024 Elektrobit Automotive GmbH
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
use common::{objects::ResourceMeasurement, std_extensions::IllegalStateResult};

pub type ResourceMeasurementReceiver = tokio::sync::mpsc::Receiver<ResourceMeasurement>;
pub type ResourceMeasurementSender = tokio::sync::mpsc::Sender<ResourceMeasurement>;

#[async_trait]
pub trait ResourceMeasurementSenderInterface {
    async fn report_resource_measurement(&self, resource_measurement: ResourceMeasurement);
}

#[async_trait]
impl ResourceMeasurementSenderInterface for ResourceMeasurementSender {
    async fn report_resource_measurement(&self, resource_measurement: ResourceMeasurement) {
        self.send(resource_measurement)
            .await
            .unwrap_or_illegal_state()
    }
}
