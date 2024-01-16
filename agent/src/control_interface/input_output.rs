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

use std::path::PathBuf;

use super::FileSystemError;
#[cfg_attr(test, mockall_double::double)]
use super::{Directory, Fifo};

pub struct InputOutput {
    input: Fifo,
    output: Fifo,
    base_dir: Directory,
}

#[cfg_attr(test, mockall::automock)]
impl InputOutput {
    // [impl->swdd~agent-control-interface-creates-two-pipes-per-workload~1]
    pub fn new(path: PathBuf) -> Result<Self, FileSystemError> {
        let input_path = path.join(String::from("input"));
        let output_path = path.join(String::from("output"));
        let base_dir = Directory::new(path)?;
        let input = Fifo::new(input_path)?;
        let output = Fifo::new(output_path)?;
        Ok(Self {
            input,
            output,
            base_dir,
        })
    }

    pub fn get_location(&self) -> PathBuf {
        self.base_dir.get_path()
    }
    pub fn get_output(&self) -> &Fifo {
        &self.output
    }
    pub fn get_input(&self) -> &Fifo {
        &self.input
    }
}

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
pub fn generate_test_input_output_mock() -> __mock_MockInputOutput::__new::Context {
    use std::path::Path;

    let input_output_mock = MockInputOutput::new_context();

    input_output_mock.expect().return_once(|path| {
        let mut mock = MockInputOutput::default();
        mock.expect_get_output().return_const({
            let mut output_fifo_mock = super::MockFifo::default();
            output_fifo_mock
                .expect_get_path()
                .return_const(Path::new("output").to_path_buf());
            output_fifo_mock.expect_drop().return_const(());
            output_fifo_mock
        });
        mock.expect_get_input().return_const({
            let mut input_fifo_mock = super::MockFifo::default();
            input_fifo_mock
                .expect_get_path()
                .return_const(Path::new("input").to_path_buf());
            input_fifo_mock.expect_drop().return_const(());
            input_fifo_mock
        });

        mock.expect_get_location().return_const(path);
        Ok(mock)
    });

    input_output_mock
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use mockall::predicate;

    use crate::control_interface::{generate_test_directory_mock, InputOutput, MockFifo};

    // [utest->swdd~agent-control-interface-creates-two-pipes-per-workload~1]
    #[test]
    fn utest_input_output_new_ok() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC.get_lock();
        let _directory_mock_context = generate_test_directory_mock("test_path", "workload_name");
        let fifo_mock_context = MockFifo::new_context();
        fifo_mock_context
            .expect()
            .with(predicate::eq(
                Path::new("test_path").join("workload_name").join("input"),
            ))
            .returning(|_| {
                let mut input_mock = MockFifo::default();
                input_mock
                    .expect_get_path()
                    .return_const(Path::new("test_path").join("workload_name").join("input"));
                input_mock.expect_drop().return_const(());
                Ok(input_mock)
            });
        fifo_mock_context
            .expect()
            .with(predicate::eq(
                Path::new("test_path").join("workload_name").join("output"),
            ))
            .return_once(|_| {
                let mut output_mock = MockFifo::default();
                output_mock
                    .expect_get_path()
                    .return_const(Path::new("test_path").join("workload_name").join("output"));
                output_mock.expect_drop().return_const(());
                Ok(output_mock)
            });

        let io = InputOutput::new(Path::new("test_path").join("workload_name"));
        assert!(io.is_ok());
        assert_eq!(
            &Path::new("test_path").join("workload_name").join("input"),
            io.as_ref().unwrap().get_input().get_path()
        );
        assert_eq!(
            &Path::new("test_path").join("workload_name").join("output"),
            io.as_ref().unwrap().get_output().get_path()
        );
        assert_eq!(
            Path::new("test_path").join("workload_name"),
            io.as_ref().unwrap().get_location()
        );
    }
}
