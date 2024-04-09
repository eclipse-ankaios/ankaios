use std::{env, fmt, process::exit, sync::Mutex};

#[cfg(not(test))]
use crossterm::terminal;
use crossterm::{cursor, style::Stylize};
#[cfg(test)]
use tests::terminal;

pub const VERBOSITY_KEY: &str = "VERBOSE";
pub const QUIET_KEY: &str = "SILENT";

static CLEANUP_STRING: Mutex<String> = Mutex::new(String::new());

#[cfg(test)]
macro_rules! println {
    ( $ ( $ arg : tt ) + ) => { $crate::log::mock_println_fn ( format_args ! ( $ ( $ arg ) + ) ) }
}

#[cfg(test)]
fn mock_println_fn(args: fmt::Arguments<'_>) {
    tests::TEST_PRINT_DATA
        .lock()
        .unwrap()
        .push_str(&args.to_string());
}

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

pub(crate) fn output_and_error_fn(args: fmt::Arguments<'_>) {
    eprintln!("{} {}", "error:".bold().red(), args);
    exit(1);
}

pub(crate) fn output_and_exit_fn(args: fmt::Arguments<'_>) {
    println!("{}", args);
    exit(0);
}

pub(crate) fn output_debug_fn(args: fmt::Arguments<'_>) {
    if is_verbose() {
        println!("{} {}{}", "debug:".blue(), args, cursor::SavePosition);
        *CLEANUP_STRING.lock().unwrap() = "".into();
    }
}

pub(crate) fn output_fn(args: fmt::Arguments<'_>) {
    if !is_quiet() {
        println!("{}{}", args, cursor::SavePosition);
        *CLEANUP_STRING.lock().unwrap() = "".into();
    }
}

pub(crate) fn output_update_fn(args: fmt::Arguments<'_>) {
    if !is_quiet() {
        let args = args.to_string();

        let terminal_width = terminal::size().unwrap_or((80, 0)).0 as usize;
        // limit line length to terminal_width by introducing newline characters
        let args = args
            .split('\n')
            .flat_map(|line| {
                line.chars()
                    .collect::<Vec<_>>()
                    .chunks(terminal_width)
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
        println!(
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
//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////
#[cfg(test)]
mod tests {
    use crossterm::cursor;
    use std::sync::Mutex;

    static TEST_PRINT_LOCK: Mutex<()> = Mutex::new(());
    pub static TEST_PRINT_DATA: Mutex<String> = Mutex::new(String::new());

    #[test]
    fn test_update_output() {
        let _x = TEST_PRINT_LOCK.lock();
        output_update!("abc\nd\nef");
        *TEST_PRINT_DATA.lock().unwrap() = String::new();
        output_update!("ABC");

        let mut expected_output = String::new();

        expected_output.push_str(&cursor::MoveToColumn(0).to_string());
        expected_output.push_str(&cursor::MoveUp(3).to_string());
        expected_output.push_str("   \n \n  \n");
        expected_output.push_str(&cursor::MoveToColumn(0).to_string());
        expected_output.push_str(&cursor::MoveUp(3).to_string());
        expected_output.push_str("ABC");

        assert_eq!(*TEST_PRINT_DATA.lock().unwrap(), expected_output);
    }

    #[test]
    fn test_update_output_normal_output_in_between() {
        let _x = TEST_PRINT_LOCK.lock();
        output_update!("abc\nd\nef");
        output!("hello");
        *TEST_PRINT_DATA.lock().unwrap() = String::new();
        output_update!("ABC");

        let mut expected_output = String::new();

        expected_output.push_str(&cursor::MoveToColumn(0).to_string());
        expected_output.push_str(&cursor::MoveToColumn(0).to_string());
        expected_output.push_str("ABC");

        assert_eq!(*TEST_PRINT_DATA.lock().unwrap(), expected_output);
    }

    #[test]
    fn test_update_output_wrap() {
        let _x = TEST_PRINT_LOCK.lock();
        output_update!("abcdefghijklm\nn\nop");
        *TEST_PRINT_DATA.lock().unwrap() = String::new();
        output_update!("abc\ndefghiklmnop\nef");

        let mut expected_output = String::new();

        expected_output.push_str(&cursor::MoveToColumn(0).to_string());
        expected_output.push_str(&cursor::MoveUp(4).to_string());
        expected_output.push_str("          \n   \n \n  \n");
        expected_output.push_str(&cursor::MoveToColumn(0).to_string());
        expected_output.push_str(&cursor::MoveUp(4).to_string());
        expected_output.push_str("abc\ndefghiklmn\nop\nef");

        assert_eq!(*TEST_PRINT_DATA.lock().unwrap(), expected_output);
    }

    pub mod terminal {
        pub fn size() -> std::io::Result<(u16, u16)> {
            Ok((10, 0))
        }
    }
}
