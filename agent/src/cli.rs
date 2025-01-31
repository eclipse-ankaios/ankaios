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

use regex::Regex;

use crate::io_utils::DEFAULT_RUN_FOLDER;
use clap::Parser;
use common::objects::STR_RE_AGENT;
use common::DEFAULT_SERVER_ADDRESS;

// [impl->swdd~agent-naming-convention~1]
fn validate_agent_name(name: &str) -> Result<String, String> {
    let re = Regex::new(STR_RE_AGENT).unwrap();
    if re.is_match(name) {
        Ok(name.to_string())
    } else {
        Err(format!(
            "Agent name '{}' is invalid. It shall contain only regular upper and lowercase characters (a-z and A-Z), numbers and the symbols '-' and '_'.",
            name
        ))
    }
}

// [impl->swdd~agent-supports-cli-argument-for-insecure-communication~1]
// [impl->swdd~agent-supports-pem-file-paths-as-cli-arguments~1]
#[derive(Parser, Debug)]
#[clap( author="The Ankaios team",
        version=env!("CARGO_PKG_VERSION"),
        about="Ankaios - your friendly automotive workload orchestrator.\nWhat can the agent do for you?")
]
pub struct Arguments {
    #[clap(short = 'n', long = "name", value_parser = clap::builder::ValueParser::new(validate_agent_name))]
    /// The name to use for the registration with the server. Every agent has to register with a unique name.
    /// Agent name shall contain only regular upper and lowercase characters (a-z and A-Z), numbers and the symbols "-" and "_".
    pub agent_name: String,
    #[clap(short = 's', long = "server-url", default_value_t = DEFAULT_SERVER_ADDRESS.to_string())]
    /// The server url.
    pub server_url: String,
    /// An existing directory where agent specific runtime files will be stored. If not specified, a default folder is created.
    #[clap(short = 'r', long = "run-folder", default_value_t = DEFAULT_RUN_FOLDER.into())]
    pub run_folder: String,
    #[clap(
        short = 'k',
        long = "insecure",
        env = "ANKAGENT_INSECURE",
        default_value_t = false
    )]
    /// Flag to disable TLS communication between Ankaios agent and server.
    pub insecure: bool,
    #[clap(long = "ca_pem", env = "ANKAGENT_CA_PEM")]
    /// Path to agent ca pem file.
    pub ca_pem: Option<String>,
    #[clap(long = "crt_pem", env = "ANKAGENT_CRT_PEM")]
    /// Path to agent certificate pem file.
    pub crt_pem: Option<String>,
    #[clap(long = "key_pem", env = "ANKAGENT_KEY_PEM")]
    /// Path to agent key pem file.
    pub key_pem: Option<String>,
}

pub fn parse() -> Arguments {
    Arguments::parse()
}

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {

    // [utest->swdd~agent-naming-convention~1]
    #[test]
    fn utest_validate_agent_name_ok() {
        assert!(super::validate_agent_name("").is_ok());

        let name = "test_AgEnt-name1_56";
        assert_eq!(super::validate_agent_name(name), Ok(name.to_string()));
    }

    // [utest->swdd~agent-naming-convention~1]
    #[test]
    fn utest_validate_agent_name_fail() {
        assert!(super::validate_agent_name("a.b").is_err());
        assert!(super::validate_agent_name("a_b_%#").is_err());
        assert!(super::validate_agent_name("a b").is_err());
    }
}
