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

use super::super::log_fetcher::{GetOutputStreams, StreamTrait};
use super::PodmanKubeWorkloadId;
use crate::runtime_connectors::runtime_connector::LogRequestOptions;

#[derive(Debug)]
pub struct PodmanKubeLogFetcher {}

impl PodmanKubeLogFetcher {
    pub fn new(_workload_id: &PodmanKubeWorkloadId, _options: &LogRequestOptions) -> Self {
        Self {}
    }
}

impl GetOutputStreams for PodmanKubeLogFetcher {
    type OutputStream = Box<dyn StreamTrait>;
    type ErrStream = Box<dyn StreamTrait>;

    fn get_output_streams(&mut self) -> (Option<Self::OutputStream>, Option<Self::ErrStream>) {
        (None, None)
    }
}
