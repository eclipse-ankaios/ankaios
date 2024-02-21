import ankaios_pb2 as ank
from google.protobuf.internal.encoder import _VarintBytes
from google.protobuf.internal.decoder import _DecodeVarint
import threading
import time
import logging

ANKAIOS_CONTROL_INTERFACE_BASE_PATH = "/run/ankaios/control_interface"
WAITING_TIME_IN_SEC = 5
REQUEST_ID = "dynamic_nginx@python_control_interface"

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

def create_request_to_add_new_workload():
    """Create the Request containing an UpdateStateRequest
    that contains the details for adding the new workload and
    the update mask to add only the new workload.
    """

    return ank.ToServer(
        request=ank.Request(
                requestId=REQUEST_ID,
                updateStateRequest=ank.UpdateStateRequest(
                    newState=ank.CompleteState(
                        format_version=ank.ApiVersion(
                                version="v0.1"
                        ),
                        desiredState=ank.State(
                                workloads={
                                    "dynamic_nginx": ank.Workload(
                                        agent="agent_A",
                                        runtime="podman",
                                        restart=True,
                                        runtimeConfig="image: docker.io/library/nginx\ncommandOptions: [\"-p\", \"8080:80\"]")
                        }
                    )
                ),
                updateMask=["desiredState.workloads.dynamic_nginx"]
            )
        )
    )

def create_request_for_complete_state():
    """Create a Request to request the CompleteState
    for querying the workload states.
    """

    return ank.ToServer(
        request=ank.Request(
            completeStateRequest=ank.CompleteStateRequest(
                fieldMask=["workloadStates"]
            ),
            requestId=REQUEST_ID,
        )
    )

def read_from_control_interface():
    """Reads from the control interface input fifo and prints the workload states."""

    with open(f"{ANKAIOS_CONTROL_INTERFACE_BASE_PATH}/input", "rb") as f:

        while True:
            varint_buffer = b'' # Buffer for reading in the byte size of the proto msg
            while True:
                next_byte = f.read(1) # Consume byte for byte
                if not next_byte:
                    break
                varint_buffer += next_byte
                if next_byte[0] & 0b10000000 == 0: # Stop if the most significant bit is 0 (indicating the last byte of the varint)
                    break
            msg_len, _ = _DecodeVarint(varint_buffer, 0) # Decode the varint and receive the proto msg length

            msg_buf = b'' # Buffer for the proto msg itself
            for _ in range(msg_len):
                next_byte = f.read(1) # Read exact amount of byte according to the calculated proto msg length
                if not next_byte:
                    break
                msg_buf += next_byte

            from_server = ank.FromServer()
            try:
                from_server.ParseFromString(msg_buf) # Deserialize the received proto msg
            except Exception as e:
                logger.info(f"Invalid response, parsing error: '{e}'")
                continue

            request_id = from_server.response.requestId
            if from_server.response.requestId == REQUEST_ID:
                logger.info(f"Receiving Response containing the workload states of the current state:\nFromServer {{\n{from_server}}}\n")
            else:
                logger.info(f"RequestId does not match. Skipping messages from requestId: {request_id}")

def write_to_control_interface():
    """Writes a Request into the control interface output fifo
    to add the new workload dynamically and every x sec according to WAITING_TIME_IN_SEC
    another Request to request the workload states.
    """

    with open(f"{ANKAIOS_CONTROL_INTERFACE_BASE_PATH}/output", "ab") as f:
        update_workload_request = create_request_to_add_new_workload()
        update_workload_request_byte_len = update_workload_request.ByteSize() # Length of the msg
        proto_update_workload_request_msg = update_workload_request.SerializeToString() # Serialized proto msg

        logger.info(f'Sending Request containing details for adding the dynamic workload \"dynamic_nginx\":\nToServer {{\n{update_workload_request}}}\n')
        f.write(_VarintBytes(update_workload_request_byte_len)) # Send the byte length of the proto msg
        f.write(proto_update_workload_request_msg) # Send the proto msg itself
        f.flush()

        request_complete_state = create_request_for_complete_state()
        request_complete_state_byte_len = request_complete_state.ByteSize() # Length of the msg
        proto_request_complete_state_msg = request_complete_state.SerializeToString() # Serialized proto msg

        while True:
            logger.info(f"Sending Request containing details for requesting all workload states:\nToServer {{{request_complete_state}}}\n")
            f.write(_VarintBytes(request_complete_state_byte_len)) # Send the byte length of the proto msg
            f.write(proto_request_complete_state_msg) # Send the proto msg itself
            f.flush()
            time.sleep(WAITING_TIME_IN_SEC) # Wait according to WAITING_TIME_IN_SEC until sending the next Request to server to avoid spamming...

if __name__ == '__main__':
    read_thread = threading.Thread(target=read_from_control_interface)
    read_thread.start()

    write_to_control_interface()

    read_thread.join()
    exit(0)
