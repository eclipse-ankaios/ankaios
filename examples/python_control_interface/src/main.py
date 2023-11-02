import ankaios_pb2 as ank
from google.protobuf.internal.encoder import _VarintBytes
from google.protobuf.internal.decoder import _DecodeVarint
import threading
import time
import logging

ANKAIOS_CONTROL_INTERFACE_BASE_PATH = "/run/ankaios/control_interface"

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

def create_update_workload_request():
    """Create the StateChangeRequest containing an UpdateStateRequest
    that contains the details for adding the new workload and
    the update mask to add only the new workload.
    """

    return ank.StateChangeRequest(
        updateState=ank.UpdateStateRequest(
                newState=ank.CompleteState(
                    currentState=ank.State(
                            workloads={
                                "dynamic_nginx": ank.Workload(
                                    agent="agent_A", 
                                    runtime="podman", 
                                    restart=True, 
                                    updateStrategy=ank.AT_MOST_ONCE, 
                                    runtimeConfig="image: docker.io/library/nginx\nports:\n- containerPort: 80\n  hostPort: 8081")
                }
            )
        ),
        updateMask=["currentState.workloads.dynamic_nginx"]
        )
    )

def create_request_complete_state_request():
    """Create a StateChangeRequest containing a RequestCompleteState
    for querying the workload states.
    """

    return ank.StateChangeRequest(
        requestCompleteState=ank.RequestCompleteState(
            requestId="request_id", 
            fieldMask=["workloadStates"]
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
            execution_request = ank.ExecutionRequest()
            execution_request.ParseFromString(msg_buf) # Deserialize the received proto msg
            logger.info(f"Receiving ExecutionRequest containing the workload states of the current state:\nExecutionRequest {{\n{execution_request}}}\n")

def write_to_control_interface():
    """Writes a StateChangeRequest into the control interface output fifo
    to add the new workload dynamically and every 30 sec another StateChangeRequest
    to request the workload states.
    """

    with open(f"{ANKAIOS_CONTROL_INTERFACE_BASE_PATH}/output", "ab") as f:
        update_workload_request = create_update_workload_request()
        update_workload_request_byte_len = update_workload_request.ByteSize() # Length of the msg
        proto_update_workload_request_msg = update_workload_request.SerializeToString() # Serialized proto msg

        logger.info(f"Sending StateChangeRequest containing details for adding the dynamic workload \'dynamic_nginx\':\nStateChangeRequest {{\n{update_workload_request}}}\n")
        f.write(_VarintBytes(update_workload_request_byte_len)) # Send the byte length of the proto msg
        f.write(proto_update_workload_request_msg) # Send the proto msg itself
        f.flush()

        request_complete_state = create_request_complete_state_request()
        request_complete_state_byte_len = request_complete_state.ByteSize() # Length of the msg
        proto_request_complete_state_msg = request_complete_state.SerializeToString() # Serialized proto msg

        while True:
            logger.info(f"Sending StateChangeRequest containing details for requesting all workload states:\nStateChangeRequest {{{request_complete_state}}}\n")
            f.write(_VarintBytes(request_complete_state_byte_len)) # Send the byte length of the proto msg
            f.write(proto_request_complete_state_msg) # Send the proto msg itself
            f.flush()
            time.sleep(30) # Wait until sending the next RequestCompleteState to avoid spamming...

if __name__ == '__main__':
    read_thread = threading.Thread(target=read_from_control_interface)
    read_thread.start()

    write_to_control_interface()

    read_thread.join()
    exit(0)
