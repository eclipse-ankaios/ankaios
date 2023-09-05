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

use std::path::Path;

#[cfg_attr(test, mockall_double::double)]
use crate::control_interface::Directory;
use crate::control_interface::FileSystemError;
use clap::Parser;
use common::DEFAULT_SERVER_ADDRESS;
use url::Url;

const DEFAULT_PODMAN_SOCK: &str = "/run/user/1000/podman/podman.sock";
const DEFAULT_RUN_FOLDER: &str = "/tmp/ankaios/";
const RUNFOLDER_SUFFIX: &str = "_io";

#[derive(Parser, Debug)]
#[clap( author="The Ankaios team", 
        version=env!("CARGO_PKG_VERSION"), 
        about="Ankaios - your friendly automotive workload orchestrator.\nWhat can the agent do for you?")
]
pub struct Arguments {
    #[clap(short = 'n', long = "name")]
    /// The name to use for the registration with the server. Every agent has to register with a unique name.
    pub agent_name: String,
    #[clap(short = 's', long = "server-url", default_value_t = DEFAULT_SERVER_ADDRESS.parse().unwrap())]
    /// The server url.
    pub server_url: Url,
    #[clap(short = 'p', long = "podman-socket-path", default_value_t = DEFAULT_PODMAN_SOCK.into())]
    /// The path to the podman socket.
    pub podman_socket_path: String,

    /// An existing path where to manage the fifo files.
    #[clap(short = 'r', long = "run-folder", default_value_t = DEFAULT_RUN_FOLDER.into())]
    pub run_folder: String,
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

    use super::*;
    use crate::control_interface::generate_test_directory_mock;

    #[test]
    fn utest_arguments_get_run_directory_use_default_directory() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC.get_lock();

        let args = Arguments {
            agent_name: "test_agent_name".to_owned(),
            podman_socket_path: "test_podman_socket_path".to_owned(),
            server_url: DEFAULT_SERVER_ADDRESS.parse().unwrap(),
            run_folder: DEFAULT_RUN_FOLDER.to_owned(),
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
            podman_socket_path: "test_podman_socket_path".to_owned(),
            server_url: DEFAULT_SERVER_ADDRESS.parse().unwrap(),
            run_folder: "/tmp/x".to_owned(),
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
