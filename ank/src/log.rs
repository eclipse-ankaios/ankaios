use std::{fmt, process::exit};

#[macro_export]
macro_rules! output_and_error {
    ( $ ( $ arg : tt ) * ) => { $crate::log::output_and_error_fn ( format_args ! ( $ ( $ arg ) * ) ) }
}

#[macro_export]
macro_rules! output_and_exit {
    ( $ ( $ arg : tt ) * ) => { $crate::log::output_and_exit_fn ( format_args ! ( $ ( $ arg ) * ) ) }
}

#[macro_export]
macro_rules! output_debug {
    ( $ ( $ arg : tt ) * ) => { $crate::log::output_debug_fn ( format_args ! ( $ ( $ arg ) * ) ) }
}

pub(crate) fn output_and_error_fn(args: fmt::Arguments<'_>) {
    eprintln!("\x1b[91m\x1b[1mERROR: {}\x1b[0m", args);
    exit(1)
}

pub(crate) fn output_and_exit_fn(args: fmt::Arguments<'_>) {
    println!("{}", args);
    exit(0);
}

pub(crate) fn output_debug_fn(args: fmt::Arguments<'_>) {
    println!("\x1b[94m{}\x1b[0m", args);
}
