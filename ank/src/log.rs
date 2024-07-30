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

use std::{env, fmt, process::exit, sync::Mutex};

use crossterm::{cursor, style::Stylize, terminal};

pub const VERBOSITY_KEY: &str = "VERBOSE";
pub const QUIET_KEY: &str = "SILENT";

static CLEANUP_STRING: Mutex<String> = Mutex::new(String::new());

/// Prints the message, if the CLI command is not called with `--quiet` flag
#[macro_export]
macro_rules! output {
    ( $ ( $ arg : tt ) + ) => { $crate::log::output_fn ( format_args ! ( $ ( $ arg ) + ) ) }
}

/// Prints the message, if the CLI command is not called with `--quiet` flag
/// If the previous text was written with this command, the old output is overwritten.
#[macro_export]
macro_rules! output_update {
    ( $ ( $ arg : tt ) + ) => { $crate::log::output_update_fn ( format_args ! ( $ ( $ arg ) + ) ) }
}

// [impl->swdd~cli-use-proprietary-tracing~1]
/// Prints the error message and immediately terminates the application with the exit code `1`.
#[macro_export]
macro_rules! output_and_error {
    ( $ ( $ arg : tt ) + ) => { $crate::log::output_and_error_fn ( format_args ! ( $ ( $ arg ) + ) ) }
}

/// Prints the message and immediately terminates the application with the exit code `0`.
#[macro_export]
macro_rules! output_and_exit {
    ( $ ( $ arg : tt ) + ) => { $crate::log::output_and_exit_fn ( format_args ! ( $ ( $ arg ) + ) ) }
}

/// This macro prints the message as a debug trace, if the CLI command is called with `--verbose` flag.
/// If the CLI command is called without the `--verbose` flag, the macro does nothing.
/// Calling this macro does not terminate the application.
#[macro_export]
macro_rules! output_debug {
    ( $ ( $ arg : tt ) + ) => { $crate::log::output_debug_fn ( format_args ! ( $ ( $ arg ) + ) ) }
}

/// This macro prints the message as a warning trace. The verbose flag has no effect on the macro.
/// Calling this macro does not terminate the application.
#[macro_export]
macro_rules! output_warn {
    ( $ ( $ arg : tt ) + ) => { $crate::log::output_warn_fn ( format_args ! ( $ ( $ arg ) + ) ) }
}

pub(crate) fn output_and_error_fn(args: fmt::Arguments<'_>) -> ! {
    eprintln!("{} {}", "error:".bold().red(), args);
    exit(1);
}

pub(crate) fn output_and_exit_fn(args: fmt::Arguments<'_>) -> ! {
    std::println!("{}", args);
    exit(0);
}

pub(crate) fn output_debug_fn(args: fmt::Arguments<'_>) {
    if is_verbose() {
        std::println!("{} {}{}", "debug:".blue(), args, cursor::SavePosition);
        *CLEANUP_STRING.lock().unwrap() = "".into();
    }
}

pub(crate) fn output_warn_fn(args: fmt::Arguments<'_>) {
    std::println!("{} {}{}", "warn:".yellow(), args, cursor::SavePosition);
    *CLEANUP_STRING.lock().unwrap() = "".into();
}

pub(crate) fn output_fn(args: fmt::Arguments<'_>) {
    if !is_quiet() {
        std::println!("{}{}", args, cursor::SavePosition);
        *CLEANUP_STRING.lock().unwrap() = "".into();
    }
}

pub fn terminal_width() -> usize {
    let terminal_width = terminal::size().unwrap_or((80, 0)).0 as usize;

    // This is a workaround for terminals that return a wrong width of 0 instead of None
    if 0 == terminal_width {
        return 80;
    }
    terminal_width
}

pub(crate) fn output_update_fn(args: fmt::Arguments<'_>) {
    if !is_quiet() {
        let args = args.to_string();

        // limit line length to terminal_width by introducing newline characters
        let args = args
            .split('\n')
            .flat_map(|line| {
                line.chars()
                    .collect::<Vec<_>>()
                    .chunks(terminal_width())
                    .map(|x| x.iter().collect::<String>())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<String>>()
            .join("\n");

        let mut cleanup_string = CLEANUP_STRING.lock().unwrap();
        let up = cleanup_string.chars().filter(|c| *c == '\n').count() as u16;
        let up_string = if up > 0 {
            cursor::MoveUp(up).to_string()
        } else {
            "".to_string()
        };
        std::println!(
            "{}{}{}{}{}{}",
            cursor::MoveToColumn(0),
            up_string,
            cleanup_string,
            cursor::MoveToColumn(0),
            up_string,
            args
        );

        let mut new_cleanup_string: String = args
            .chars()
            .map(|x| if x == '\n' { '\n' } else { ' ' })
            .collect();
        new_cleanup_string.push('\n');
        *cleanup_string = new_cleanup_string;
    }
}

fn is_verbose() -> bool {
    matches!(env::var(VERBOSITY_KEY), Ok(verbose) if verbose.to_lowercase() == "true")
        && !is_quiet()
}

fn is_quiet() -> bool {
    matches!(env::var(QUIET_KEY), Ok(quiet) if quiet.to_lowercase() == "true")
}
