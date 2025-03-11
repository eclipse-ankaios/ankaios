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

use common::objects::WorkloadInstanceName;
use std::{ops::Deref, path::PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub struct ControlInterfacePath(PathBuf);
const SUBFOLDER_CONTROL_INTERFACE: &str = "control_interface";

impl ControlInterfacePath {
    pub fn new(path: PathBuf) -> Self {
        Self(path)
    }
}

impl Deref for ControlInterfacePath {
    type Target = PathBuf;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<(&PathBuf, &WorkloadInstanceName)> for ControlInterfacePath {
    fn from((run_folder, workload_instance_name): (&PathBuf, &WorkloadInstanceName)) -> Self {
        let control_interface_path = workload_instance_name
            .pipes_folder_name(run_folder.as_path())
            .join(SUBFOLDER_CONTROL_INTERFACE);
        Self(control_interface_path)
    }
}

impl PartialEq<PathBuf> for ControlInterfacePath {
    fn eq(&self, other: &PathBuf) -> bool {
        self.0 == *other
    }
}
