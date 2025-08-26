# Copyright (c) 2023 Elektrobit Automotive GmbH
#
# This program and the accompanying materials are made available under the
# terms of the Apache License, Version 2.0 which is available at
# https://www.apache.org/licenses/LICENSE-2.0.
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
# WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
# License for the specific language governing permissions and limitations
# under the License.
#
# SPDX-License-Identifier: Apache-2.0

# =======================================================================
# Imports
# =======================================================================
# - ank_base_pb2 and control_api_pb2 are generated from the protobuf files.
#     They are required to create and parse the protobuf messages.
# - _VarintBytes and _DecodeVarint are used to handle the variable-length
#     encoding of protobuf messages.
import ank_base_pb2 as ank_base
import control_api_pb2 as control_api
from google.protobuf.internal.encoder import _VarintBytes
from google.protobuf.internal.decoder import _DecodeVarint
import os, time, logging
import threading


# =======================================================================
# Variables - general
# =======================================================================
# - ANKAIOS_CONTROL_INTERFACE_BASE_PATH is the path to the input and output
#     FIFO files of the control interface.
# - WAITING_TIME_IN_SEC represents the default waiting time.
# - UPDATE_STATE_REQUEST_ID is the request ID for the update state request.
# - COMPLETE_STATE_REQUEST_ID is the request ID for the complete state request.
# - PROTOCOL_VERSION is the version of Ankaios, which for this example
#     is set to the value of the environment variable 'ANKAIOS_VERSION'.
# - CONNECTED is a flag to check the connection is established.
# - CONNECTION_CLOSED is a flag to check if Ankaios closed the connection.
ANKAIOS_CONTROL_INTERFACE_BASE_PATH = "/run/ankaios/control_interface"
WAITING_TIME_IN_SEC = 5
UPDATE_STATE_REQUEST_ID = "dynamic_nginx@12345"
COMPLETE_STATE_REQUEST_ID = "dynamic_nginx@67890"
PROTOCOL_VERSION = os.environ.get('ANKAIOS_VERSION')
CONNECTED = False
CONNECTION_CLOSED = False


# =======================================================================
# Setup logger
# =======================================================================
def create_logger():
    """Create a logger with custom format and default log level."""
    formatter = logging.Formatter('%(asctime)s %(message)s', datefmt="%FT%TZ")
    logger = logging.getLogger("custom_logger")
    handler = logging.StreamHandler()
    handler.setFormatter(formatter)
    logger.addHandler(handler)
    logger.setLevel(logging.INFO)
    return logger

logger = create_logger()


# =======================================================================
# Variables - messages
# =======================================================================
# - PROTO_HELLO_MESSAGE is the initial required message to establish
#     a connection with Ankaios.
# - PROTO_UPDATE_STATE_REQUEST is used to update the state of the cluster.
#     In this example, it is used to add a new workload dynamically. It
#     contains the details for adding the new workload and the update mask
#     to add only the new workload.
# - PROTO_COMPLETE_STATE_REQUEST is used to request the CompleteState for
#     querying the state of the dynamic_nginx workload.
PROTO_HELLO_MESSAGE = control_api.ToAnkaios (
    hello=control_api.Hello(
        protocolVersion=PROTOCOL_VERSION,
    ),
)

PROTO_UPDATE_STATE_REQUEST = control_api.ToAnkaios(
    request=ank_base.Request(
        requestId=UPDATE_STATE_REQUEST_ID,
        updateStateRequest=ank_base.UpdateStateRequest(
            newState=ank_base.CompleteState(
                desiredState=ank_base.State(
                    apiVersion="v0.1",
                    workloads=ank_base.WorkloadMap(workloads={
                        "dynamic_nginx": ank_base.Workload(
                            agent="agent_A",
                            runtime="podman",
                            restartPolicy=ank_base.NEVER,
                            runtimeConfig="image: docker.io/library/nginx\ncommandOptions: [\"-p\", \"8080:80\"]")
                    })
                )
            ),
            updateMask=["desiredState.workloads.dynamic_nginx"]
        )
    )
)

PROTO_COMPLETE_STATE_REQUEST = control_api.ToAnkaios(
    request=ank_base.Request(
        completeStateRequest=ank_base.CompleteStateRequest(
            fieldMask=["workloadStates.agent_A.dynamic_nginx"]
        ),
        requestId=COMPLETE_STATE_REQUEST_ID,
    )
)


# =======================================================================
# Ankaios control interface methods
# =======================================================================
# - read_protobuf_data reads the protobuf message from the control interface
#     input fifo.
# - read_from_control_interface continuously reads from the control interface
#     input fifo and sends the response to be handled.
# - handle_response processes the response from Ankaios. It checks the type
#     of the response and handles it accordingly.
# - write_to_control_interface writes a ToAnkaios message to the control
#     interface output fifo.
def read_protobuf_data(file_handle):
    """Reads a protobuf message from the control interface input fifo."""
    varint_buffer = b'' # Buffer for reading in the byte size of the proto msg
    while True:
        next_byte = file_handle.read(1) # Consume byte for byte
        if not next_byte:
            break
        varint_buffer += next_byte
        if next_byte[0] & 0b10000000 == 0: # Stop if the most significant bit is 0 (indicating the last byte of the varint)
            break
    msg_len, _ = _DecodeVarint(varint_buffer, 0) # Decode the varint and receive the proto msg length

    msg_buf = b'' # Buffer for the proto msg itself
    for _ in range(msg_len):
        next_byte = file_handle.read(1) # Read exact amount of byte according to the calculated proto msg length
        if not next_byte:
            break
        msg_buf += next_byte
    return msg_buf


def read_from_control_interface():
    """Reads from the control interface input fifo and handles the responses."""
    global CONNECTION_CLOSED
    with open(f"{ANKAIOS_CONTROL_INTERFACE_BASE_PATH}/input", "rb") as file_handle:
        while not CONNECTION_CLOSED:
            from_ankaios = control_api.FromAnkaios()  # Prepare the FromAnkaios object
            msg_buf = read_protobuf_data(file_handle)  # Read the protobuf message from the input fifo
            try:
                from_ankaios.ParseFromString(msg_buf) # Deserialize the received proto msg
            except Exception as e:
                logger.error(f"Invalid response, parsing error: '{e}'")
                continue

            handle_response(from_ankaios)  # Handle the response from Ankaios


def handle_response(from_ankaios: control_api.FromAnkaios):
    """Handles the response from Ankaios."""
    global CONNECTED, CONNECTION_CLOSED

    # Check if the connection has been established or not
    if not CONNECTED:
        if from_ankaios.HasField("controlInterfaceAccepted"):
            logger.info("Received Control interface accepted response.")
            CONNECTED = True
        elif from_ankaios.HasField("connectionClosed"):
            logger.info("Received Connection Closed response. Exiting..")
            CONNECTION_CLOSED = True
        else:
            logger.info("Received unknown message type before connection established. Skipping message..")

    # Connection is established, handle the response accordingly
    else:
        if from_ankaios.HasField("response"):
            request_id = from_ankaios.response.requestId
            if from_ankaios.response.requestId == UPDATE_STATE_REQUEST_ID:
                # Extract the workload instance names from the response
                added_workloads = from_ankaios.response.UpdateStateSuccess.addedWorkloads
                deleted_workloads = from_ankaios.response.UpdateStateSuccess.deletedWorkloads
                logger.info("Received Response for the UpdateStateRequest:\n"
                            f"added workloads: {added_workloads}, deleted workloads: {deleted_workloads}")
            elif from_ankaios.response.requestId == COMPLETE_STATE_REQUEST_ID:
                logger.info(f"Received Response for the CompleteStateRequest: \n{from_ankaios.response.completeState}")
            else:
                logger.info(f"RequestId does not match. Skipping messages from requestId: {request_id}")
        elif from_ankaios.HasField("connectionClosed"):
            logger.info("Received Connection Closed response. Exiting..")
            CONNECTION_CLOSED = True
            CONNECTED = False
        else:
            logger.info("Received unknown message type. Skipping message.")


def write_to_control_interface(file_handle, message: control_api.ToAnkaios):
    """Writes a ToAnkaios message to the control interface output fifo."""
    message_len = message.ByteSize()  # Length of the msg
    message_buffer = message.SerializeToString()  # Serialized proto msg

    file_handle.write(_VarintBytes(message_len))  # Send the byte length of the proto msg
    file_handle.write(message_buffer)  # Send the proto msg itself
    file_handle.flush()  # Flush to ensure immediate delivery


# =======================================================================
# Main
# =======================================================================
if __name__ == '__main__':
    # Assure the control interface fifo files exist
    assert os.path.exists(f"{ANKAIOS_CONTROL_INTERFACE_BASE_PATH}/output"), "Output FIFO does not exist."
    assert os.path.exists(f"{ANKAIOS_CONTROL_INTERFACE_BASE_PATH}/input"), "Input FIFO does not exist."

    # Start the reading thread
    read_thread = threading.Thread(target=read_from_control_interface)
    read_thread.start()

    with open(f"{ANKAIOS_CONTROL_INTERFACE_BASE_PATH}/output", "ab") as output_file:
        # Send hello message to establish the connection
        logger.info("Sending initial Hello message to establish connection...")
        write_to_control_interface(output_file, PROTO_HELLO_MESSAGE)
        time.sleep(1)  # Give some time for the connection to be established
        assert CONNECTED, "Connection to Ankaios not established."

        # Send the request to add the dynamic_nginx workload
        logger.info("Requesting to add the dynamic_nginx workload...")
        write_to_control_interface(output_file, PROTO_UPDATE_STATE_REQUEST)
        time.sleep(0.1)  # Give some time for the request to be processed

        while CONNECTED:
            # Send the request for the complete state
            logger.info("Requesting complete state of the dynamic_nginx workload...")
            write_to_control_interface(output_file, PROTO_COMPLETE_STATE_REQUEST)
            time.sleep(WAITING_TIME_IN_SEC)

        # Wait for the reading thread to finish
        read_thread.join()

    exit(0)
