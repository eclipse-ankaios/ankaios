#[cfg(test)]
mod grpc_tests {

    use std::time::Duration;

    use common::{
        commands::{self, CompleteState, CompleteStateRequest, Request, RequestContent},
        communications_client::CommunicationsClient,
        communications_error::CommunicationMiddlewareError,
        communications_server::CommunicationsServer,
        from_server_interface::FromServer,
        to_server_interface::{ToServer, ToServerInterface},
    };
    use grpc::{client::GRPCCommunicationsClient, server::GRPCCommunicationsServer};

    use tokio::{
        sync::mpsc::{Receiver, Sender},
        time::timeout,
    };
    use url::Url;

    enum CommunicationType {
        Cli,
        Agent,
    }

    async fn generate_test_grpc_communication_client(
        server_addr: &str,
        comm_type: CommunicationType,
        test_request_id: &str,
        to_grpc_server: Sender<FromServer>,
    ) -> (
        Sender<ToServer>,
        tokio::task::JoinHandle<Result<(), CommunicationMiddlewareError>>,
    ) {
        let (to_grpc_client, grpc_client_receiver) = tokio::sync::mpsc::channel::<ToServer>(20);
        let url = Url::parse(&format!("http://{}", server_addr)).expect("error");
        let mut grpc_communications_client = match comm_type {
            CommunicationType::Cli => {
                GRPCCommunicationsClient::new_cli_communication(test_request_id.to_owned(), url)
            }
            CommunicationType::Agent => {
                GRPCCommunicationsClient::new_agent_communication(test_request_id.to_owned(), url)
            }
        };

        let grpc_client_task = tokio::spawn(async move {
            grpc_communications_client
                .run(grpc_client_receiver, to_grpc_server)
                .await
        });

        (to_grpc_client, grpc_client_task)
    }

    async fn generate_test_grpc_communication_setup(
        port: u16,
        comm_type: CommunicationType,
        test_request_id: &str,
    ) -> (
        Sender<ToServer>,                                                  // to_grpc_client
        Receiver<ToServer>,                                                // server_receiver
        tokio::task::JoinHandle<Result<(), CommunicationMiddlewareError>>, // grpc_server_task
        tokio::task::JoinHandle<Result<(), CommunicationMiddlewareError>>, // grpc_client_task
    ) {
        ///////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////
        //                                         _____________                                _________________
        //                                        |             | -----grpc over http--------> |    0.0.0.0:port |
        //  test_case ------->to_grpc_client----->| grpc_client |                              |    grpc_server  |
        //                                        |_____________|                              |_________________|
        //                                                                                              |---to_server---> server_receiver
        //
        //////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////

        let server_addr = format!("0.0.0.0:{}", port);
        let (to_grpc_server, grpc_server_receiver) = tokio::sync::mpsc::channel::<FromServer>(20);
        let (to_server, server_receiver) = tokio::sync::mpsc::channel::<ToServer>(20);

        // create communication server
        let mut communications_server = GRPCCommunicationsServer::new(to_server);

        let socket_addr: std::net::SocketAddr = server_addr.parse().unwrap();

        let grpc_server_task = tokio::spawn(async move {
            communications_server
                .start(grpc_server_receiver, socket_addr)
                .await
        });

        // create communication client
        let (to_grpc_client, grpc_client_task) = generate_test_grpc_communication_client(
            &server_addr,
            comm_type,
            test_request_id,
            to_grpc_server,
        )
        .await;

        (
            to_grpc_client,
            server_receiver,
            grpc_server_task,
            grpc_client_task,
        )
    }

    // [itest->swdd~grpc-server-provides-endpoint-for-cli-connection-handling~1]
    // [itest->swdd~grpc-server-creates-cli-connection~1]
    #[tokio::test(flavor = "multi_thread", worker_threads = 1)] // set worker_threads = 1 to solve the failing of the test on woodpecker
    async fn itest_grpc_communication_client_cli_connection_grpc_server_received_request_complete_state(
    ) {
        let test_request_id = "test_request_id";
        let (to_grpc_client, mut server_receiver, _, _) =
            generate_test_grpc_communication_setup(25551, CommunicationType::Cli, test_request_id)
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
        let (to_grpc_client, mut server_receiver, _, _) =
            generate_test_grpc_communication_setup(50052, CommunicationType::Cli, test_request_id)
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

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)] // set worker_threads = 1 to solve the failing of the test on woodpecker
    async fn itest_grpc_communication_client_agent_connection_grpc_server_received_agent_hello() {
        let test_agent_name = "test_agent_name";
        let (_, mut server_receiver, _, _) = generate_test_grpc_communication_setup(
            50053,
            CommunicationType::Agent,
            test_agent_name,
        )
        .await;

        let result = timeout(Duration::from_millis(10000), server_receiver.recv()).await;

        assert!(matches!(
            result,
            Ok(Some(ToServer::AgentHello(commands::AgentHello { agent_name }))) if agent_name == test_agent_name
        ));
    }
}
