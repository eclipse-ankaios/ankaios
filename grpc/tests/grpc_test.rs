#[cfg(test)]
mod grpc_tests {

    use std::time::Duration;

    use common::{
        commands::{self, CompleteState, RequestCompleteState},
        communications_client::CommunicationsClient,
        communications_server::CommunicationsServer,
        execution_interface::ExecutionCommand,
        state_change_interface::{StateChangeCommand, StateChangeInterface},
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
        to_grpc_server: Sender<ExecutionCommand>,
    ) -> (Sender<StateChangeCommand>, tokio::task::JoinHandle<()>) {
        let (to_grpc_client, grpc_client_receiver) =
            tokio::sync::mpsc::channel::<StateChangeCommand>(20);
        let url = Url::parse(&format!("http://{}", server_addr)).expect("error");
        let mut grps_communications_client = match comm_type {
            CommunicationType::Cli => {
                GRPCCommunicationsClient::new_cli_communication(test_request_id.to_owned(), url)
            }
            CommunicationType::Agent => {
                GRPCCommunicationsClient::new_agent_communication(test_request_id.to_owned(), url)
            }
        };

        let grpc_client_task = tokio::spawn(async move {
            grps_communications_client
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
        Sender<StateChangeCommand>,   // to_grpc_client
        Receiver<StateChangeCommand>, // server_receiver
        tokio::task::JoinHandle<()>,  // grpc_server_task
        tokio::task::JoinHandle<()>,  // grpc_client_task
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
        let (to_grpc_server, mut grpc_server_receiver) =
            tokio::sync::mpsc::channel::<ExecutionCommand>(20);
        let (to_server, server_receiver) = tokio::sync::mpsc::channel::<StateChangeCommand>(20);

        // create communication server
        let mut communications_server = GRPCCommunicationsServer::new(to_server);

        let socket_addr: std::net::SocketAddr = server_addr.parse().unwrap();

        let grpc_server_task = tokio::spawn(async move {
            communications_server
                .start(&mut grpc_server_receiver, socket_addr)
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
            generate_test_grpc_communication_setup(50051, CommunicationType::Cli, test_request_id)
                .await;

        // send request to grpc client
        to_grpc_client
            .request_complete_state(RequestCompleteState {
                request_id: test_request_id.to_owned(),
                field_mask: vec![],
            })
            .await;

        // read request forwarded by grpc communication server
        let result = timeout(Duration::from_millis(3000), server_receiver.recv()).await;

        assert!(matches!(
            result,
            Ok(Some(StateChangeCommand::RequestCompleteState(
                RequestCompleteState {
                    request_id,
                    field_mask
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
        to_grpc_client
            .update_state(
                CompleteState {
                    request_id: test_request_id.to_owned(),
                    ..Default::default()
                },
                vec![],
            )
            .await;

        // read request forwarded by grpc communication server
        let result = timeout(Duration::from_millis(3000), server_receiver.recv()).await;

        assert!(matches!(
            result,
            Ok(Some(StateChangeCommand::UpdateState(update_state_request))
            ) if update_state_request.state.request_id == test_request_id
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
            Ok(Some(StateChangeCommand::AgentHello(commands::AgentHello { agent_name }))) if agent_name == test_agent_name
        ));
    }
}
