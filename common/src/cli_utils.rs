use crate::{SERVER_ADDRESS_ENV_KEY, SERVER_URL_ENV_KEY};
use clap::error::{ContextKind, ContextValue, ErrorKind};
use std::{env, net::SocketAddr, str::FromStr};
use url::Url;

fn create_error_context(
    cmd: &clap::Command,
    env_key: &str,
    arg: Option<&clap::Arg>,
    arg_value: &String,
) -> clap::Error {
    let mut err = clap::Error::new(ErrorKind::ValueValidation).with_cmd(cmd); // the order in which the errors are inserted is important
    if let Some(arg) = arg {
        if let Ok(env_value) = env::var(env_key) {
            if env_value == *arg_value {
                err.insert(
                    ContextKind::InvalidArg,
                    ContextValue::String(format!("environment variable '{}'", env_key)),
                );
            } else {
                err.insert(
                    ContextKind::InvalidArg,
                    ContextValue::String(arg.to_string()),
                );
            }
        } else {
            err.insert(
                ContextKind::InvalidArg,
                ContextValue::String(arg.to_string()),
            );
        }
    }

    err.insert(
        ContextKind::InvalidValue,
        ContextValue::String(arg_value.clone()),
    );
    err
}

#[derive(Clone)]
/// Custom url parser as a workaround to Clap's bug about
/// outputting the wrong error message context
/// when using the environment variable and not the cli argument.
/// When using a wrong value inside environment variable
/// Clap still outputs that the cli argument was wrongly set,
/// but not the environment variable. This is poor use-ability.
/// An issue for this bug is already opened (https://github.com/clap-rs/clap/issues/5202).
/// The code will be removed if the bug in Clap is fixed.
pub struct ServerUrlParser;

impl clap::builder::TypedValueParser for ServerUrlParser {
    type Value = Url;

    fn parse_ref(
        &self,
        cmd: &clap::Command,
        arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, clap::Error> {
        let arg_value = value.to_string_lossy().to_string();
        let err = create_error_context(cmd, SERVER_URL_ENV_KEY, arg, &arg_value);
        let url = Url::from_str(&arg_value).map_err(|_| err)?;
        Ok(url)
    }
}

#[derive(Clone)]
/// Custom url parser as a workaround to Clap's bug about
/// outputting the wrong error message context
/// when using the environment variable and not the cli argument.
/// When using a wrong value inside environment variable
/// Clap still outputs that the cli argument was wrongly set,
/// but not the environment variable. This is poor use-ability.
/// An issue for this bug is already opened (https://github.com/clap-rs/clap/issues/5202).
/// The code will be removed if the bug in Clap is fixed.
pub struct ServerAddressParser;

impl clap::builder::TypedValueParser for ServerAddressParser {
    type Value = SocketAddr;

    fn parse_ref(
        &self,
        cmd: &clap::Command,
        arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, clap::Error> {
        let arg_value = value.to_string_lossy().to_string();
        let err = create_error_context(cmd, SERVER_ADDRESS_ENV_KEY, arg, &arg_value);
        let url = SocketAddr::from_str(&arg_value).map_err(|_| err)?;
        Ok(url)
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
mod tests {
    use super::*;
    use clap::builder::TypedValueParser;
    use mockall::lazy_static;
    use std::env;
    use std::ffi::OsStr;
    const INVALID_VALUE: &str = "invalid-value";
    const PROGRAM_NAME: &str = "some program";
    const ARG_NAME: &str = "arg";
    const EXAMPLE_URL: &str = "http://0.0.0.0:11111";
    const EXAMPLE_SOCKET_ADDRESS: &str = "0.0.0.0:11111";

    lazy_static! {
        pub static ref MOCKALL_CONTEXT_SYNC: common::test_utils::MockAllContextSync =
            common::test_utils::MockAllContextSync::new();
    }

    struct CleanupEnv;

    impl Drop for CleanupEnv {
        fn drop(&mut self) {
            env::remove_var(common::SERVER_URL_ENV_KEY);
            env::remove_var(common::SERVER_ADDRESS_ENV_KEY);
        }
    }

    #[test]
    fn utest_cli_argument_server_url_use_cli_arg() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock();
        let _cleanup = CleanupEnv;
        let url_parser = ServerUrlParser;
        let cmd = clap::Command::new(PROGRAM_NAME);
        let arg = clap::Arg::new(ARG_NAME);
        let actual_url = url_parser
            .parse_ref(&cmd, Some(&arg), OsStr::new(crate::DEFAULT_SERVER_ADDRESS))
            .unwrap();
        let expected_url = Url::from_str(crate::DEFAULT_SERVER_ADDRESS).unwrap();
        assert_eq!(actual_url, expected_url);
    }

    #[test]
    fn utest_cli_argument_server_url_use_env_var() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock();
        let _cleanup = CleanupEnv;
        let url_parser = ServerUrlParser;
        let cmd = clap::Command::new(PROGRAM_NAME);
        let arg = clap::Arg::new(ARG_NAME);
        std::env::set_var(crate::SERVER_URL_ENV_KEY, EXAMPLE_URL);
        let actual_url = url_parser
            .parse_ref(&cmd, Some(&arg), OsStr::new(EXAMPLE_URL))
            .unwrap();

        let expected_url = Url::from_str(EXAMPLE_URL).unwrap();
        assert_eq!(actual_url, expected_url);
    }

    #[test]
    fn utest_cli_argument_server_url_prioritize_cli_arg() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock();
        let _cleanup = CleanupEnv;
        let url_parser = ServerUrlParser;
        let cmd = clap::Command::new(PROGRAM_NAME);
        let arg = clap::Arg::new(ARG_NAME);
        std::env::set_var(crate::SERVER_URL_ENV_KEY, EXAMPLE_URL);
        let actual_url = url_parser
            .parse_ref(&cmd, Some(&arg), OsStr::new(crate::DEFAULT_SERVER_ADDRESS))
            .unwrap();

        let expected_url = Url::from_str(crate::DEFAULT_SERVER_ADDRESS).unwrap();
        assert_eq!(actual_url, expected_url);
    }

    #[test]
    fn utest_cli_argument_server_url_use_env_var_error_context() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock();
        let _cleanup = CleanupEnv;
        let url_parser = ServerUrlParser;
        let cmd = clap::Command::new(PROGRAM_NAME);
        let arg = clap::Arg::new(ARG_NAME);
        std::env::set_var(crate::SERVER_URL_ENV_KEY, INVALID_VALUE);
        let parsing_result = url_parser.parse_ref(&cmd, Some(&arg), OsStr::new(INVALID_VALUE));

        assert!(parsing_result
            .err()
            .unwrap()
            .to_string()
            .contains("environment variable"));
    }

    #[test]
    fn utest_cli_argument_server_url_use_cli_arg_error_context() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock();
        let _cleanup = CleanupEnv;
        let url_parser = ServerUrlParser;
        let cmd = clap::Command::new(PROGRAM_NAME);
        let arg = clap::Arg::new(ARG_NAME);
        let parsing_result = url_parser.parse_ref(&cmd, Some(&arg), OsStr::new(INVALID_VALUE));
        assert!(parsing_result.err().unwrap().to_string().contains(ARG_NAME));
    }

    #[test]
    fn utest_cli_argument_server_url_none_arg_error_context() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock();
        let _cleanup = CleanupEnv;
        let url_parser = ServerUrlParser;
        let cmd = clap::Command::new(PROGRAM_NAME);
        let parsing_result = url_parser.parse_ref(&cmd, None, OsStr::new(INVALID_VALUE));
        let err: String = parsing_result.err().unwrap().to_string();
        assert!(err.contains("invalid value for one of the arguments"));
    }

    #[test]
    fn utest_cli_argument_server_address_use_cli_arg() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock();
        let _cleanup = CleanupEnv;
        let url_parser = ServerAddressParser;
        let cmd = clap::Command::new(PROGRAM_NAME);
        let arg = clap::Arg::new(ARG_NAME);
        let actual_socket_addr = url_parser
            .parse_ref(&cmd, Some(&arg), OsStr::new(crate::DEFAULT_SOCKET_ADDRESS))
            .unwrap();
        let expected_socket_addr = SocketAddr::from_str(crate::DEFAULT_SOCKET_ADDRESS).unwrap();
        assert_eq!(actual_socket_addr, expected_socket_addr);
    }

    #[test]
    fn utest_cli_argument_server_address_use_env_var() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock();
        let _cleanup = CleanupEnv;
        let url_parser = ServerAddressParser;
        let cmd = clap::Command::new(PROGRAM_NAME);
        let arg = clap::Arg::new(ARG_NAME);
        std::env::set_var(crate::SERVER_ADDRESS_ENV_KEY, EXAMPLE_SOCKET_ADDRESS);
        let actual_socket_addr = url_parser
            .parse_ref(&cmd, Some(&arg), OsStr::new(EXAMPLE_SOCKET_ADDRESS))
            .unwrap();

        let expected_socket_addr = SocketAddr::from_str(EXAMPLE_SOCKET_ADDRESS).unwrap();
        assert_eq!(actual_socket_addr, expected_socket_addr);
    }

    #[test]
    fn utest_cli_argument_server_address_prioritize_cli_arg() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock();
        let _cleanup = CleanupEnv;
        let url_parser = ServerAddressParser;
        let cmd = clap::Command::new(PROGRAM_NAME);
        let arg = clap::Arg::new(ARG_NAME);
        std::env::set_var(crate::SERVER_ADDRESS_ENV_KEY, EXAMPLE_SOCKET_ADDRESS);
        let actual_socket_addr = url_parser
            .parse_ref(&cmd, Some(&arg), OsStr::new(crate::DEFAULT_SOCKET_ADDRESS))
            .unwrap();

        let expected_socket_addr = SocketAddr::from_str(crate::DEFAULT_SOCKET_ADDRESS).unwrap();
        assert_eq!(actual_socket_addr, expected_socket_addr);
    }

    #[test]
    fn utest_cli_argument_server_address_use_env_var_error_context() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock();
        let _cleanup = CleanupEnv;
        let url_parser = ServerAddressParser;
        let cmd = clap::Command::new(PROGRAM_NAME);
        let arg = clap::Arg::new(ARG_NAME);
        std::env::set_var(crate::SERVER_ADDRESS_ENV_KEY, INVALID_VALUE);
        let parsing_result = url_parser.parse_ref(&cmd, Some(&arg), OsStr::new(INVALID_VALUE));

        assert!(parsing_result
            .err()
            .unwrap()
            .to_string()
            .contains("environment variable"));
    }

    #[test]
    fn utest_cli_argument_server_address_use_cli_arg_error_context() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock();
        let _cleanup = CleanupEnv;
        let url_parser = ServerAddressParser;
        let cmd = clap::Command::new(PROGRAM_NAME);
        let arg = clap::Arg::new(ARG_NAME);
        let parsing_result = url_parser.parse_ref(&cmd, Some(&arg), OsStr::new(INVALID_VALUE));
        assert!(parsing_result.err().unwrap().to_string().contains(ARG_NAME));
    }

    #[test]
    fn utest_cli_argument_server_address_none_arg_error_context() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock();
        let _cleanup = CleanupEnv;
        let url_parser = ServerAddressParser;
        let cmd = clap::Command::new(PROGRAM_NAME);
        let parsing_result = url_parser.parse_ref(&cmd, None, OsStr::new(INVALID_VALUE));
        let err: String = parsing_result.err().unwrap().to_string();
        assert!(err.contains("invalid value for one of the arguments"));
    }
}
