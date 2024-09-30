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
use std::path::Path;

#[cfg_attr(test, mockall_double::double)]
use crate::control_interface::Directory;
use crate::control_interface::FileSystemError;
use clap::Parser;
use common::objects::state::STR_RE_AGENT;
use common::DEFAULT_SERVER_ADDRESS;

const DEFAULT_RUN_FOLDER: &str = "/tmp/ankaios/";
const RUNFOLDER_SUFFIX: &str = "_io";

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
    /// An existing path where to manage the fifo files.
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

impl Arguments {
    pub fn get_run_directory(&self) -> Result<Directory, FileSystemError> {
        let base_folder = Path::new(&self.run_folder);
        let run_folder = base_folder.join(format!("{}{}", self.agent_name, RUNFOLDER_SUFFIX));
        if base_folder.to_str() != Some(DEFAULT_RUN_FOLDER) && !run_folder.exists() {
            return Err(FileSystemError::NotFoundDirectory(
                run_folder.into_os_string(),
            ));
        }

        Directory::new(run_folder)
    }
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
    use common::DEFAULT_SERVER_ADDRESS;

    use super::{Arguments, FileSystemError, Path, DEFAULT_RUN_FOLDER};
    use crate::control_interface::generate_test_directory_mock;

    #[test]
    fn utest_arguments_get_run_directory_use_default_directory() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC.get_lock();

        let args = Arguments {
            agent_name: "test_agent_name".to_owned(),
            server_url: DEFAULT_SERVER_ADDRESS.to_string(),
            run_folder: DEFAULT_RUN_FOLDER.to_owned(),
            insecure: true,
            ca_pem: None,
            crt_pem: None,
            key_pem: None,
        };

        let _directory_mock_context =
            generate_test_directory_mock(DEFAULT_RUN_FOLDER, "test_agent_name_io");

        assert!(args.get_run_directory().is_ok());
    }

    #[test]
    fn utest_arguments_get_run_directory_given_directory_not_found() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC.get_lock();

        let args = Arguments {
            agent_name: "test_agent_name".to_owned(),
            server_url: DEFAULT_SERVER_ADDRESS.to_string(),
            run_folder: "/tmp/x".to_owned(),
            insecure: true,
            ca_pem: None,
            crt_pem: None,
            key_pem: None,
        };

        let _directory_mock_context = generate_test_directory_mock("/tmp/x", "test_agent_name_io");

        assert_eq!(
            args.get_run_directory(),
            Err(FileSystemError::NotFoundDirectory(
                Path::new("/tmp/x/test_agent_name_io")
                    .as_os_str()
                    .to_os_string()
            ))
        );
    }
}
