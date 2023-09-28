use std::{
    env::{self, VarError},
    fmt,
    process::exit,
};

pub const VERBOSITY_KEY: &str = "VERBOSE";

/// Prints the error message and immediately terminates the application with the exit code 1.
#[macro_export]
macro_rules! output_and_error {
    ( $ ( $ arg : tt ) * ) => { $crate::log::output_and_error_fn ( format_args ! ( $ ( $ arg ) * ) ) }
}

/// Prints the message and immediately terminates the application with the exit code 0.
#[macro_export]
macro_rules! output_and_exit {
    ( $ ( $ arg : tt ) * ) => { $crate::log::output_and_exit_fn ( format_args ! ( $ ( $ arg ) * ) ) }
}

/// Prints the message as a debug trace. It does not terminate the application (the application continues).
#[macro_export]
macro_rules! output_debug {
    ( $ ( $ arg : tt ) * ) => { $crate::log::output_debug_fn ( format_args ! ( $ ( $ arg ) * ) ) }
}

pub(crate) fn output_and_error_fn(args: fmt::Arguments<'_>) {
    eprintln!("\x1b[91m\x1b[1mERROR: {}\x1b[0m", args);
    exit(1);
}

pub(crate) fn output_and_exit_fn(args: fmt::Arguments<'_>) {
    println!("{}", args);
    exit(0);
}

pub(crate) fn output_debug_fn(args: fmt::Arguments<'_>) {
    if let Ok(verbose) = is_verbose() {
        if verbose {
            println!("\x1b[94mDEBUG: {}\x1b[0m", args);
        }
    }
}

fn is_verbose() -> Result<bool, VarError> {
    Ok(env::var(VERBOSITY_KEY)?.to_lowercase() == "true")
}
