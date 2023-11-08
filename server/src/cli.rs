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

use clap::{
    error::{ContextKind, ContextValue, ErrorKind},
    Parser,
};
use common::{DEFAULT_SOCKET_ADDRESS, SERVER_ADDRESS_ENV_KEY};
use std::{env, net::SocketAddr, str::FromStr};

pub fn parse() -> Arguments {
    Arguments::parse()
}

#[derive(Clone, Default)]
/// Custom url parser as a workaround to Clap's bug about
/// outputting the wrong error message context
/// when using the environment variable and not the cli argument.
/// When using a wrong value inside environment variable
/// Clap still outputs that the cli argument was wrongly set,
/// but not the environment variable. This is poor use-ability.
/// An issue for this bug is already opened (https://github.com/clap-rs/clap/issues/5202).
/// The code will be removed if the bug in Clap is fixed.
struct ServerAddressParser;

impl clap::builder::TypedValueParser for ServerAddressParser {
    type Value = SocketAddr;

    fn parse_ref(
        &self,
        cmd: &clap::Command,
        arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, clap::Error> {
        let mut err = clap::Error::new(ErrorKind::ValueValidation).with_cmd(cmd);
        if let Some(arg) = arg {
            if let Ok(env_var) = std::env::var(SERVER_ADDRESS_ENV_KEY) {
                if env_var == value.to_string_lossy() {
                    err.insert(
                        ContextKind::InvalidArg,
                        ContextValue::String(format!(
                            "environment variable '{}'",
                            SERVER_ADDRESS_ENV_KEY
                        )),
                    );
                } else {
                    err.insert(
                        ContextKind::InvalidArg,
                        ContextValue::String(arg.to_string()),
                    );
                }
            } else {
                err.insert(
                    ContextKind::InvalidArg,
                    ContextValue::String(arg.to_string()),
                );
            }
        }

        err.insert(
            ContextKind::InvalidValue,
            ContextValue::String(value.to_str().unwrap().to_owned()),
        );
        let url = SocketAddr::from_str(value.to_str().unwrap()).map_err(|_| err)?;
        Ok(url)
    }
}

#[derive(Parser, Debug)]
#[clap( author="The Ankaios team", 
        version=env!("CARGO_PKG_VERSION"), 
        about="Ankaios - your friendly automotive workload orchestrator.\nWhat can the server do for you?")]
pub struct Arguments {
    #[clap(short = 'c', long = "startup-config")]
    /// The path to the startup config yaml.
    pub path: String,
    #[clap(short = 'a', long = "address", default_value_t = DEFAULT_SOCKET_ADDRESS.parse().unwrap(), env = SERVER_ADDRESS_ENV_KEY, value_parser = ServerAddressParser::default())]
    /// The address, including the port, the server shall listen at.
    pub addr: SocketAddr,
}
// Note: this code is intentionally without unit tests.
// There is no business logic which can be tested, here we have only a config and a call of "clap" crate.
