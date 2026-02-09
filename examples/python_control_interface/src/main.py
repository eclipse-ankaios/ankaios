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


import ank_base_pb2 as ank_base
import control_api_pb2 as control_api
from google.protobuf.internal.encoder import _VarintBytes
from google.protobuf.internal.decoder import _DecodeVarint
import os
import time
import logging


ANKAIOS_CONTROL_INTERFACE_BASE_PATH = "/run/ankaios/control_interface"
WAITING_TIME_IN_SEC = 5
TIMEOUT_IN_SEC = 1
UPDATE_STATE_REQUEST_ID = "RWNsaXBzZSBBbmthaW9z"
COMPLETE_STATE_REQUEST_ID = "QW5rYWlvcyBpcyB0aGUgYmVzdA=="
PROTOCOL_VERSION = os.environ.get('ANKAIOS_VERSION')


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

PROTO_WORKLOAD_STATE_REQUEST = control_api.ToAnkaios(
    request=ank_base.Request(
        completeStateRequest=ank_base.CompleteStateRequest(
            fieldMask=["workloadStates.agent_A.dynamic_nginx"]
        ),
        requestId=COMPLETE_STATE_REQUEST_ID,
    )
)


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


def read_from_control_interface(file_handle) -> control_api.FromAnkaios:
    """Reads from the control interface input fifo and returns the response"""
    from_ankaios = control_api.FromAnkaios()  # Prepare the FromAnkaios object
    msg_buf = read_protobuf_data(file_handle)  # Read the protobuf message from the input fifo
    try:
        from_ankaios.ParseFromString(msg_buf) # Deserialize the received proto msg
    except Exception as e:
        logger.error(f"Invalid response, parsing error: '{e}'")
        return None

    return from_ankaios


def write_to_control_interface(file_handle, message: control_api.ToAnkaios):
    """Writes a ToAnkaios message to the control interface output fifo."""
    message_len = message.ByteSize()  # Length of the msg
    message_buffer = message.SerializeToString()  # Serialized proto msg

    file_handle.write(_VarintBytes(message_len))  # Send the byte length of the proto msg
    file_handle.write(message_buffer)  # Send the proto msg itself
    file_handle.flush()  # Flush to ensure immediate delivery


if __name__ == '__main__':
    # Assure the control interface fifo files exist
    assert os.path.exists(f"{ANKAIOS_CONTROL_INTERFACE_BASE_PATH}/output"), "Output FIFO does not exist."
    assert os.path.exists(f"{ANKAIOS_CONTROL_INTERFACE_BASE_PATH}/input"), "Input FIFO does not exist."

    # Open file for writing to the control interface and send the initial Hello message
    output_file = open(f"{ANKAIOS_CONTROL_INTERFACE_BASE_PATH}/output", "ab")
    logger.info("Sending initial Hello message to establish connection...")
    write_to_control_interface(output_file, PROTO_HELLO_MESSAGE)

    # Open file for writing to the control interface and check for the response
    input_file = open(f"{ANKAIOS_CONTROL_INTERFACE_BASE_PATH}/input", "rb")
    response = read_from_control_interface(input_file)
    assert response.HasField("controlInterfaceAccepted"), "Response should have been ControlInterfaceAccepted"
    logger.info(f"Receiving answer to the initial Hello:\n{response}" )

    logger.info("Requesting to add the dynamic_nginx workload...")
    write_to_control_interface(output_file, PROTO_UPDATE_STATE_REQUEST)
    response = read_from_control_interface(input_file)
    assert response.HasField("response"), "Response should contain a response field"
    assert response.response.HasField("UpdateStateSuccess"), "Response should be of type UpdateStateSuccess"
    assert response.response.requestId == UPDATE_STATE_REQUEST_ID, f"Response requestId should be {UPDATE_STATE_REQUEST_ID}"
    logger.info(f"Received response for the UpdateStateRequest:\n{response}")

    while not output_file.closed and not input_file.closed:
        logger.info("Requesting workload state of the dynamic_nginx workload...")
        write_to_control_interface(output_file, PROTO_WORKLOAD_STATE_REQUEST)
        response = read_from_control_interface(input_file)
        assert response.HasField("response"), "Response should contain a response field"
        assert response.response.HasField("completeStateResponse"), "Response should be of type CompleteStateResponse"
        assert response.response.requestId == COMPLETE_STATE_REQUEST_ID, f"Response requestId should be {COMPLETE_STATE_REQUEST_ID}"
        logger.info(f"Receiving response for the CompleteStateRequest with filter 'workloadStates.agent_A.dynamic_nginx':\n{response}")
        time.sleep(WAITING_TIME_IN_SEC)

    # CLose the file handles
    output_file.close()
    input_file.close()

    exit(0)
