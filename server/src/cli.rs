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

use clap::Parser;
use common::DEFAULT_SOCKET_ADDRESS;
use std::{env, net::SocketAddr};

pub fn parse() -> Arguments {
    Arguments::parse()
}

#[derive(Parser, Debug)]
#[clap( author="The Ankaios team", 
        version=env!("CARGO_PKG_VERSION"), 
        about="Ankaios - your friendly automotive workload orchestrator.\nWhat can the server do for you?")]
pub struct Arguments {
    #[clap(short = 'c', long = "startup-config")]
    /// The path to the startup config yaml.
    pub path: String,
    #[clap(short = 'a', long = "address", default_value_t = DEFAULT_SOCKET_ADDRESS.parse().unwrap())]
    /// The address, including the port, the server shall listen at.
    pub addr: SocketAddr,
}
// Note: this code is intentionally without unit tests.
// There is no business logic which can be tested, here we have only a config and a call of "clap" crate.
