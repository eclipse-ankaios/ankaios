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

pub trait ExitGracefully<T, E> {
    fn unwrap_or_exit_func<F>(self, op: F, exit_code: i32) -> T
    where
        F: FnOnce(E);

    fn unwrap_or_exit(self, message: &str) -> T;
}

impl<T, E: std::fmt::Display> ExitGracefully<T, E> for Result<T, E> {
    /// Returns the contained [`Ok`] value or executes the closure and
    /// exits the program gracefully with the provided exit code.
    ///
    /// # Examples
    ///
    /// ```should_panic
    /// use common::graceful_exit::ExitGracefully;
    /// fn exit(x: &str) { eprintln!("Failed."); }
    ///
    /// assert_eq!(Ok::<&str, &str>("foo").unwrap_or_exit_func(exit, 1), "foo");
    ///
    /// // shall exit program gracefully with log message "Failed." and exit code 1
    /// Err::<&str, &str>("some error").unwrap_or_exit_func(exit, 1);
    /// ```
    fn unwrap_or_exit_func<F>(self, op: F, exit_code: i32) -> T
    where
        F: FnOnce(E),
    {
        match self {
            Ok(value) => value,
            Err(error) => {
                op(error);
                std::process::exit(exit_code);
            }
        }
    }

    /// Returns the contained [`Ok`] value or exits the program gracefully
    /// with an error log and exit code 1.
    ///
    /// # Examples
    ///
    /// ```should_panic
    /// use common::graceful_exit::ExitGracefully;
    /// assert_eq!(Ok::<&str, &str>("foo").unwrap_or_exit("Expected 2"), "foo");
    ///
    /// // shall exit program gracefully with log message "Expected 2: some error" and exit code 1
    /// Err::<&str, &str>("some error").unwrap_or_exit("Expected 2");
    /// ```
    fn unwrap_or_exit(self, message: &str) -> T {
        match self {
            Ok(value) => value,
            Err(error) => {
                log::error!(target: Default::default(), "{message}: {error}");
                std::process::exit(1);
            }
        }
    }
}
