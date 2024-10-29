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

mod cli_command;

mod podman_cli;

pub(crate) mod podman;

pub(crate) mod podman_kube;

mod runtime_connector;
pub use runtime_connector::{
    OwnableRuntime, ReusableWorkloadState, RuntimeConnector, RuntimeError,
};

#[cfg(test)]
pub use runtime_connector::test;

mod runtime_facade;
pub use runtime_facade::{GenericRuntimeFacade, RuntimeFacade};

#[cfg(test)]
pub use runtime_facade::MockRuntimeFacade;

mod state_checker;
pub use state_checker::{RuntimeStateGetter, StateChecker};

#[cfg(test)]
pub use state_checker::MockRuntimeStateGetter;
