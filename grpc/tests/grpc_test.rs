#[cfg(test)]
mod grpc_tests {

    use std::{
        fs::File,
        io::{self, Write},
        os::unix::fs::PermissionsExt,
        path::PathBuf,
        time::Duration,
    };

    use common::{
        commands::{self, CompleteStateRequest, Request, RequestContent},
        communications_client::CommunicationsClient,
        communications_error::CommunicationMiddlewareError,
        communications_server::CommunicationsServer,
        from_server_interface::{FromServer, FromServerSender},
        objects::CompleteState,
        to_server_interface::{ToServer, ToServerInterface, ToServerReceiver, ToServerSender},
    };
    use grpc::{
        client::GRPCCommunicationsClient,
        security::{self, TLSConfig},
        server::GRPCCommunicationsServer,
    };

    use tempfile::TempDir;
    use tokio::time::timeout;

    static TEST_CA_PEM_CONTENT: &str = r#"-----BEGIN CERTIFICATE-----
MIHkMIGXAhR1zkvNFbDRYcMkCksHtdg+rJKhiDAFBgMrZXAwFTETMBEGA1UEAwwK
YW5rYWlvcy1jYTAeFw0yNDA3MTYxMjI3MTlaFw0yNDA4MTUxMjI3MTlaMBUxEzAR
BgNVBAMMCmFua2Fpb3MtY2EwKjAFBgMrZXADIQCp/t2l56QoKjkNshjF/V/RGOuw
WrUnN97zRglDPLo1/zAFBgMrZXADQQABjHOWypeg/IqsAKfrrzeMgKbnIrah19of
uP/v/vU+YcQKX+D6RhuSQ8j16/+EAOjvtaOK5dhYOr586A9RkRkG
-----END CERTIFICATE-----"#;

    static TEST_SERVER_CRT_PEM_CONTENT: &str = r#"-----BEGIN CERTIFICATE-----
MIIBdzCCASmgAwIBAgIULY8Ctn8ms/ZG14b2gY3dD/YlBOgwBQYDK2VwMBUxEzAR
BgNVBAMMCmFua2Fpb3MtY2EwHhcNMjQwNzE2MTIyNzE5WhcNMjQwODE1MTIyNzE5
WjAVMRMwEQYDVQQDDAphbmstc2VydmVyMCowBQYDK2VwAyEAQsBvTo+Aguw/1V3v
PdRIPBLOX/5LehxynZCF9RYO342jgYowgYcwFQYDVR0RBA4wDIIKYW5rLXNlcnZl
cjATBgNVHSUEDDAKBggrBgEFBQcDATAdBgNVHQ4EFgQUFDnQo+PrhS1/bBBDuyj2
szOg76IwOgYDVR0jBDMwMaEZpBcwFTETMBEGA1UEAwwKYW5rYWlvcy1jYYIUdc5L
zRWw0WHDJApLB7XYPqySoYgwBQYDK2VwA0EAJw/NudyP/OURnqrswsQFOmsa0k2K
XgPaaa8TMfsTlvYpqryUgBh4ExU3BkjMCAQ9IELhlHSY0EVm9tup4NfBCw==
-----END CERTIFICATE-----"#;

    static TEST_SERVER_KEY_PEM_CONTENT: &str = r#"-----BEGIN PRIVATE KEY-----
MC4CAQAwBQYDK2VwBCIEIIRQaWZdo6rAdOazCEQqjyvf3HtJqXOs4NMofTh6SAzx
-----END PRIVATE KEY-----"#;

    static TEST_AGENT_CRT_PEM_CONTENT: &str = r#"-----BEGIN CERTIFICATE-----
MIIBajCCARygAwIBAgIUL44KMvvhoo5ZrSaokgW5bKEJLrQwBQYDK2VwMBUxEzAR
BgNVBAMMCmFua2Fpb3MtY2EwHhcNMjQwNzE2MTIyNzE5WhcNMjQwODE1MTIyNzE5
WjASMRAwDgYDVQQDDAdhZ2VudF9BMCowBQYDK2VwAyEAmKIygp9agnovfVFfPkPH
yeSr4HesbiXCEVPCeY+2y4qjgYAwfjAMBgNVHREEBTADggEqMBMGA1UdJQQMMAoG
CCsGAQUFBwMCMB0GA1UdDgQWBBR08ZG+pI5lJ81sEAjT0ikK+iSIvjA6BgNVHSME
MzAxoRmkFzAVMRMwEQYDVQQDDAphbmthaW9zLWNhghR1zkvNFbDRYcMkCksHtdg+
rJKhiDAFBgMrZXADQQC5kORmC3HMaTdjMFHE8EnqDBsBK+SVXZ8IM/Gd4yc7OaoV
/7JPXB5vLd7jCMRb3cprrjdmJbwCdqCkZVVAcw4D
-----END CERTIFICATE-----"#;

    static TEST_AGENT_KEY_PEM_CONTENT: &str = r#"-----BEGIN PRIVATE KEY-----
MC4CAQAwBQYDK2VwBCIEIB3zzTRSmz4iMoa1uYyOfiY8g0p33CP73eVKXqfsy6V4
-----END PRIVATE KEY-----"#;

    static TEST_CLI_CRT_PEM_CONTENT: &str = r#"-----BEGIN CERTIFICATE-----
MIIBaTCCARugAwIBAgIUTRcuJFpGqhGd9pc3fZsT9Upi1X0wBQYDK2VwMBUxEzAR
BgNVBAMMCmFua2Fpb3MtY2EwHhcNMjQwNzE2MTIyNzE5WhcNMjQwODE1MTIyNzE5
WjAOMQwwCgYDVQQDDANjbGkwKjAFBgMrZXADIQCT88OAtJh0C3ItFseYeIRCmxfH
gNLfvojEcjEo70uZEaOBgzCBgDAOBgNVHREEBzAFggNjbGkwEwYDVR0lBAwwCgYI
KwYBBQUHAwIwHQYDVR0OBBYEFNuw0WHOTueZYpyV9Xt8ROld52W8MDoGA1UdIwQz
MDGhGaQXMBUxEzARBgNVBAMMCmFua2Fpb3MtY2GCFHXOS80VsNFhwyQKSwe12D6s
kqGIMAUGAytlcANBAP+3ZZ8micEqh8q+3PjGwF16bUZf3UmakLu40nu0LcUglCBq
7+PtdepuCUHArbPswCgUebanY8zvH1mVkv1FPAw=
-----END CERTIFICATE-----"#;

    static TEST_CLI_KEY_PEM_CONTENT: &str = r#"-----BEGIN PRIVATE KEY-----
MC4CAQAwBQYDK2VwBCIEIKjThmghW/8MJ64v7FooHHKdx5chlf4d7Rtff/YHQWDX
-----END PRIVATE KEY-----"#;

    pub struct TestPEMFilesPackage {
        // The directory and everything inside it will be automatically deleted once the returned TempDir is destroyed.
        pub _working_dir: TempDir,
        pub ca_pem_file_path: PathBuf,
        pub server_pem_file_path: PathBuf,
        pub server_key_pem_file_path: PathBuf,
        pub agent_pem_file_path: PathBuf,
        pub agent_key_pem_file_path: PathBuf,
        pub cli_pem_file_path: PathBuf,
        pub cli_key_pem_file_path: PathBuf,
    }

    impl TestPEMFilesPackage {
        pub fn new() -> Result<Self, io::Error> {
            let working_dir = TempDir::new()?;
            let ca_pem_file_path = working_dir.path().join("ca.pem");
            let mut ca_pem_file = File::create(ca_pem_file_path.as_path())?;
            ca_pem_file.write_all(TEST_CA_PEM_CONTENT.as_bytes())?;
            // ensure that all in-memory data reaches the filesystem before returning to prevent probable concurrency issues.
            ca_pem_file.sync_all()?;

            let server_pem_file_path = working_dir.path().join("server.pem");
            let mut server_pem_file = File::create(server_pem_file_path.as_path())?;
            server_pem_file.write_all(TEST_SERVER_CRT_PEM_CONTENT.as_bytes())?;
            server_pem_file.sync_all()?;

            let server_key_pem_file_path = working_dir.path().join("server-key.pem");
            let mut server_key_pem_file = File::create(server_key_pem_file_path.as_path())?;
            server_key_pem_file.write_all(TEST_SERVER_KEY_PEM_CONTENT.as_bytes())?;
            let mut server_key_permissions = server_key_pem_file.metadata()?.permissions();
            server_key_permissions.set_mode(0o600);
            let _ = server_key_pem_file.set_permissions(server_key_permissions);
            server_key_pem_file.sync_all()?;

            let agent_pem_file_path = working_dir.path().join("agent.pem");
            let mut agent_pem_file = File::create(agent_pem_file_path.as_path())?;
            agent_pem_file.write_all(TEST_AGENT_CRT_PEM_CONTENT.as_bytes())?;
            agent_pem_file.sync_all()?;

            let agent_key_pem_file_path = working_dir.path().join("agent-key.pem");
            let mut agent_key_pem_file = File::create(agent_key_pem_file_path.as_path())?;
            agent_key_pem_file.write_all(TEST_AGENT_KEY_PEM_CONTENT.as_bytes())?;
            let mut agent_key_permissions = agent_key_pem_file.metadata()?.permissions();
            agent_key_permissions.set_mode(0o600);
            let _ = agent_key_pem_file.set_permissions(agent_key_permissions);
            agent_key_pem_file.sync_all()?;

            let cli_pem_file_path = working_dir.path().join("cli.pem");
            let mut cli_pem_file = File::create(cli_pem_file_path.as_path())?;
            cli_pem_file.write_all(TEST_CLI_CRT_PEM_CONTENT.as_bytes())?;
            cli_pem_file.sync_all()?;

            let cli_key_pem_file_path = working_dir.path().join("cli-key.pem");
            let mut cli_key_pem_file = File::create(cli_key_pem_file_path.as_path())?;
            cli_key_pem_file.write_all(TEST_CLI_KEY_PEM_CONTENT.as_bytes())?;
            let mut cli_key_permissions = cli_key_pem_file.metadata()?.permissions();
            cli_key_permissions.set_mode(0o600);
            let _ = cli_key_pem_file.set_permissions(cli_key_permissions);
            cli_key_pem_file.sync_all()?;

            Ok(Self {
                _working_dir: working_dir,
                ca_pem_file_path,
                server_pem_file_path,
                server_key_pem_file_path,
                agent_pem_file_path,
                agent_key_pem_file_path,
                cli_pem_file_path,
                cli_key_pem_file_path,
            })
        }

        pub fn get_server_tls_config(&self) -> TLSConfig {
            TLSConfig {
                path_to_ca_pem: self
                    .ca_pem_file_path
                    .clone()
                    .into_os_string()
                    .into_string()
                    .unwrap(),
                path_to_crt_pem: self
                    .server_pem_file_path
                    .clone()
                    .into_os_string()
                    .into_string()
                    .unwrap(),
                path_to_key_pem: self
                    .server_key_pem_file_path
                    .clone()
                    .into_os_string()
                    .into_string()
                    .unwrap(),
            }
        }
        pub fn get_agent_tls_config(&self) -> TLSConfig {
            TLSConfig {
                path_to_ca_pem: self
                    .ca_pem_file_path
                    .clone()
                    .into_os_string()
                    .into_string()
                    .unwrap(),
                path_to_crt_pem: self
                    .agent_pem_file_path
                    .clone()
                    .into_os_string()
                    .into_string()
                    .unwrap(),
                path_to_key_pem: self
                    .agent_key_pem_file_path
                    .clone()
                    .into_os_string()
                    .into_string()
                    .unwrap(),
            }
        }
        pub fn get_cli_tls_config(&self) -> TLSConfig {
            TLSConfig {
                path_to_ca_pem: self
                    .ca_pem_file_path
                    .clone()
                    .into_os_string()
                    .into_string()
                    .unwrap(),
                path_to_crt_pem: self
                    .cli_pem_file_path
                    .clone()
                    .into_os_string()
                    .into_string()
                    .unwrap(),
                path_to_key_pem: self
                    .cli_key_pem_file_path
                    .clone()
                    .into_os_string()
                    .into_string()
                    .unwrap(),
            }
        }
    }

    enum CommunicationType {
        Cli,
        Agent,
    }

    async fn generate_test_grpc_communication_client(
        server_addr: &str,
        comm_type: &CommunicationType,
        test_request_id: &str,
        to_grpc_server: FromServerSender,
        tls_config: Option<security::TLSConfig>,
    ) -> (
        ToServerSender,
        tokio::task::JoinHandle<Result<(), CommunicationMiddlewareError>>,
    ) {
        let (to_grpc_client, grpc_client_receiver) = tokio::sync::mpsc::channel::<ToServer>(20);
        let url = format!("http://{}", server_addr);
        let grpc_communications_client = match comm_type {
            CommunicationType::Cli => GRPCCommunicationsClient::new_cli_communication(
                test_request_id.to_owned(),
                url,
                tls_config,
            ),
            CommunicationType::Agent => GRPCCommunicationsClient::new_agent_communication(
                test_request_id.to_owned(),
                url,
                tls_config,
            ),
        };

        let grpc_client_task = tokio::spawn(async move {
            grpc_communications_client?
                .run(grpc_client_receiver, to_grpc_server)
                .await
        });

        (to_grpc_client, grpc_client_task)
    }

    async fn generate_test_grpc_communication_setup(
        port: u16,
        comm_type: CommunicationType,
        test_request_id: &str,
        tls_pem_files_package: Option<&TestPEMFilesPackage>,
    ) -> (
        ToServerSender,                                                    // to_grpc_client
        ToServerReceiver,                                                  // server_receiver
        tokio::task::JoinHandle<Result<(), CommunicationMiddlewareError>>, // grpc_server_task
        tokio::task::JoinHandle<Result<(), CommunicationMiddlewareError>>, // grpc_client_task
    ) {
        ///////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////
        //                                         _____________                                _________________
        //                                        |             | -----grpc over http(s)--------> |    0.0.0.0:port |
        //  test_case ------->to_grpc_client----->| grpc_client |                              |    grpc_server  |
        //                                        |_____________|                              |_________________|
        //                                                                                              |---to_server---> server_receiver
        //
        //////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////

        let server_addr = format!("0.0.0.0:{}", port);
        let (to_grpc_server, grpc_server_receiver) = tokio::sync::mpsc::channel::<FromServer>(20);
        let (to_server, server_receiver) = tokio::sync::mpsc::channel::<ToServer>(20);

        let (server_tls_config, agent_tls_config, cli_tls_config) =
            if let Some(tls_pem_files_package) = tls_pem_files_package {
                (
                    Some(tls_pem_files_package.get_server_tls_config()),
                    Some(tls_pem_files_package.get_agent_tls_config()),
                    Some(tls_pem_files_package.get_cli_tls_config()),
                )
            } else {
                (None, None, None)
            };

        // create communication server
        let mut communications_server = GRPCCommunicationsServer::new(to_server, server_tls_config);

        let socket_addr: std::net::SocketAddr = server_addr.parse().unwrap();

        let grpc_server_task = tokio::spawn(async move {
            communications_server
                .start(grpc_server_receiver, socket_addr)
                .await
        });

        // create communication client
        let (to_grpc_client, grpc_client_task) = generate_test_grpc_communication_client(
            &server_addr,
            &comm_type,
            test_request_id,
            to_grpc_server,
            match comm_type {
                CommunicationType::Agent => agent_tls_config,
                CommunicationType::Cli => cli_tls_config,
            },
        )
        .await;

        (
            to_grpc_client,
            server_receiver,
            grpc_server_task,
            grpc_client_task,
        )
    }

    // [itest->swdd~grpc-server-activate-mtls-when-certificates-and-key-provided-upon-start~1]
    // [itest->swdd~grpc-cli-activate-mtls-when-certificates-and-key-provided-upon-start~1]
    #[tokio::test(flavor = "multi_thread", worker_threads = 1)] // set worker_threads = 1 to solve the failing of the test on woodpecker
    async fn itest_grpc_communication_client_cli_connection_grpc_server_received_request_complete_state_with_tls(
    ) {
        let test_request_id = "test_request_id";
        let test_pem_files_package = TestPEMFilesPackage::new().unwrap();

        let (to_grpc_client, mut server_receiver, _, _) = generate_test_grpc_communication_setup(
            25551,
            CommunicationType::Cli,
            test_request_id,
            Some(&test_pem_files_package),
        )
        .await;

        // send request to grpc client
        let request_complete_state_result = to_grpc_client
            .request_complete_state(
                test_request_id.to_owned(),
                CompleteStateRequest { field_mask: vec![] },
            )
            .await;

        assert!(request_complete_state_result.is_ok());

        // read request forwarded by grpc communication server
        let result = timeout(Duration::from_millis(3000), server_receiver.recv()).await;

        println!("result: {:?}", result);

        assert!(matches!(
            result,
            Ok(Some(ToServer::Request(
                Request{
                    request_id,
                    request_content: RequestContent::CompleteStateRequest(CompleteStateRequest {
                        field_mask
                    })
                }
            ))) if request_id.contains(test_request_id) && field_mask.is_empty()
        ));
    }

    // [itest->swdd~grpc-server-provides-endpoint-for-cli-connection-handling~1]
    // [itest->swdd~grpc-server-creates-cli-connection~1]
    // [itest->swdd~grpc-server-deactivate-mtls-when-no-certificates-and-no-key-provided-upon-start~1]
    // [itest->swdd~grpc-cli-deactivate-mtls-when-no-certificates-and-no-key-provided-upon-start~1]
    #[tokio::test(flavor = "multi_thread", worker_threads = 1)] // set worker_threads = 1 to solve the failing of the test on woodpecker
    async fn itest_grpc_communication_client_cli_connection_grpc_server_received_request_complete_state(
    ) {
        let test_request_id = "test_request_id";
        let (to_grpc_client, mut server_receiver, _, _) = generate_test_grpc_communication_setup(
            25551,
            CommunicationType::Cli,
            test_request_id,
            None,
        )
        .await;

        // send request to grpc client
        let request_complete_state_result = to_grpc_client
            .request_complete_state(
                test_request_id.to_owned(),
                CompleteStateRequest { field_mask: vec![] },
            )
            .await;

        assert!(request_complete_state_result.is_ok());

        // read request forwarded by grpc communication server
        let result = timeout(Duration::from_millis(3000), server_receiver.recv()).await;

        assert!(matches!(
            result,
            Ok(Some(ToServer::Request(
                Request{
                    request_id,
                    request_content: RequestContent::CompleteStateRequest(CompleteStateRequest {
                        field_mask
                    })
                }
            ))) if request_id.contains(test_request_id) && field_mask.is_empty()
        ));
    }

    // [itest->swdd~grpc-server-provides-endpoint-for-cli-connection-handling~1]
    // [itest->swdd~grpc-server-creates-cli-connection~1]
    #[tokio::test(flavor = "multi_thread", worker_threads = 1)] // set worker_threads = 1 to solve the failing of the test on woodpecker
    async fn itest_grpc_communication_client_cli_connection_grpc_server_received_update_state() {
        let test_request_id = "test_request_id";
        let (to_grpc_client, mut server_receiver, _, _) = generate_test_grpc_communication_setup(
            50052,
            CommunicationType::Cli,
            test_request_id,
            None,
        )
        .await;

        // send request to grpc client
        let update_state_result = to_grpc_client
            .update_state(
                test_request_id.to_owned(),
                CompleteState {
                    ..Default::default()
                },
                vec![],
            )
            .await;
        assert!(update_state_result.is_ok());

        // read request forwarded by grpc communication server
        let result = timeout(Duration::from_millis(3000), server_receiver.recv()).await;

        assert!(matches!(
            result,
            Ok(Some(ToServer::Request(Request{request_id, request_content: _}))
            ) if request_id.contains(test_request_id)
        ));
    }

    // [itest->swdd~grpc-agent-deactivate-mtls-when-no-certificates-and-no-key-provided-upon-start~1]
    #[tokio::test(flavor = "multi_thread", worker_threads = 1)] // set worker_threads = 1 to solve the failing of the test on woodpecker
    async fn itest_grpc_communication_client_agent_connection_grpc_server_received_agent_hello() {
        let test_agent_name = "test_agent_name";
        let (_, mut server_receiver, _, _) = generate_test_grpc_communication_setup(
            50053,
            CommunicationType::Agent,
            test_agent_name,
            None,
        )
        .await;

        let result = timeout(Duration::from_millis(10000), server_receiver.recv()).await;

        assert!(matches!(
            result,
            Ok(Some(ToServer::AgentHello(commands::AgentHello { agent_name }))) if agent_name == test_agent_name
        ));
    }

    // [itest->swdd~grpc-agent-activate-mtls-when-certificates-and-key-provided-upon-start~1]
    #[tokio::test(flavor = "multi_thread", worker_threads = 1)] // set worker_threads = 1 to solve the failing of the test on woodpecker
    async fn itest_grpc_communication_client_agent_connection_grpc_server_received_agent_hello_with_tls(
    ) {
        let test_agent_name = "test_agent_name";
        let test_pem_files_package = TestPEMFilesPackage::new().unwrap();

        let (_, mut server_receiver, _, _) = generate_test_grpc_communication_setup(
            50053,
            CommunicationType::Agent,
            test_agent_name,
            Some(&test_pem_files_package),
        )
        .await;

        let result = timeout(Duration::from_millis(10000), server_receiver.recv()).await;

        assert!(matches!(
            result,
            Ok(Some(ToServer::AgentHello(commands::AgentHello { agent_name }))) if agent_name == test_agent_name
        ));
    }
}
