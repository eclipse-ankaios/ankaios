use std::{env, fmt, process::exit};

pub const VERBOSITY_KEY: &str = "VERBOSE";

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

pub(crate) fn output_and_error_fn(args: fmt::Arguments<'_>) {
    eprintln!("\x1b[31m\x1b[1merror:\x1b[0m {}", args);
    exit(1);
}

pub(crate) fn output_and_exit_fn(args: fmt::Arguments<'_>) {
    println!("{}", args);
    exit(0);
}

pub(crate) fn output_debug_fn(args: fmt::Arguments<'_>) {
    if is_verbose() {
        println!("\x1b[94mdebug:\x1b[0m {}", args);
    }
}

fn is_verbose() -> bool {
    matches!(env::var(VERBOSITY_KEY), Ok(verbose) if verbose.to_lowercase() == "true")
}
