#include <iostream>
#include <fstream>
#include <thread>
#include <chrono>
#include <iomanip>
#include "src/proto/ankaios.pb.h"
#include <google/protobuf/io/coded_stream.h>
#include <google/protobuf/util/delimited_message_util.h>
#include <google/protobuf/io/zero_copy_stream_impl.h>

static const std::string ANKAIOS_CONTROL_INTERFACE_BASE_PATH{"/run/ankaios/control_interface"};
static const int WAITING_TIME_IN_SEC { 5 };

namespace logging
{
    /* Log function that logs various arguments 
        to the specified stream in a custom log format. */
    template <typename... Msgs>
    void log(std::ostream &stream, Msgs &&...msgs)
    {
        std::stringstream message;
        ((message << msgs), ...);
        const auto currentTime = std::chrono::system_clock::to_time_t(std::chrono::system_clock::now());
        stream << '[' << std::put_time(std::localtime(&currentTime), "%FT%TZ") << "] " << message.str() << std::endl;
    }
}

/* Create the StateChangeRequest containing an UpdateStateRequest
    that contains the details for adding the new workload and
    the update mask to add only the new workload. */
ankaios::StateChangeRequest createUpdateWorkloadRequest()
{
    ankaios::Workload newWorkload;
    newWorkload.set_agent("agent_A");
    newWorkload.set_runtime("podman");
    newWorkload.set_restart(true);
    newWorkload.set_updatestrategy(ankaios::UpdateStrategy::AT_MOST_ONCE);
    newWorkload.set_runtimeconfig("image: docker.io/library/nginx\ncommandOptions: [\"-p\", \"8080:80\"]");

    ankaios::State *state{new ankaios::State};
    state->mutable_workloads()->insert({"dynamic_nginx", std::move(newWorkload)});

    ankaios::CompleteState *completeState{new ankaios::CompleteState};
    completeState->set_allocated_currentstate(state);

    ankaios::UpdateStateRequest *updateStateRequest{new ankaios::UpdateStateRequest};
    updateStateRequest->set_allocated_newstate(completeState);
    updateStateRequest->add_updatemask("currentState.workloads.dynamic_nginx");

    ankaios::StateChangeRequest stateChangeRequest;
    stateChangeRequest.set_allocated_updatestate(updateStateRequest);
    return stateChangeRequest;
}

/* Create a StateChangeRequest containing a RequestCompleteState
    for querying the workload states. */
ankaios::StateChangeRequest createRequestCompleteStateRequest()
{
    ankaios::RequestCompleteState *requestCompleteState{new ankaios::RequestCompleteState};
    requestCompleteState->set_requestid("request_id");
    requestCompleteState->add_fieldmask("workloadStates");

    ankaios::StateChangeRequest stateChangeRequest;
    stateChangeRequest.set_allocated_requestcompletestate(requestCompleteState);

    return stateChangeRequest;
}

/* Writes a StateChangeRequest into the control interface output fifo 
    to add the new workload dynamically and every 30 sec another StateChangeRequest
    to request the workload states. */
void readFromControlInterface()
{
    const auto inputFifo = ANKAIOS_CONTROL_INTERFACE_BASE_PATH + "/input";

    std::ifstream input{inputFifo, std::ios::in | std::ios::binary};

    if (input.fail())
    {
        logging::log(std::cerr, "Error: could not open file ", inputFifo);
        return;
    }

    const int BLOCK_SIZE = 1; // disable blocking I/O by setting block_size to 1 byte instead of default value
    google::protobuf::io::IstreamInputStream bufferedInputStream{&input, BLOCK_SIZE};

    bool result = true;
    do
    {
        ankaios::ExecutionRequest executionRequest;
        bool clean_eof = false;
        // read length-delimited protobuf message to output the workload states
        result = google::protobuf::util::ParseDelimitedFromZeroCopyStream(&executionRequest, &bufferedInputStream, &clean_eof);
        if (result)
        {
            logging::log(std::cout,
                         "Receiving ExecutionRequest containing the workload states of the current state:\n",
                         "ExecutionRequest {\n",
                         executionRequest.DebugString(),
                         "}\n");
        }
    } while (result);
}

void writeToControlInterface()
{
    const auto updateWorkloadState = createUpdateWorkloadRequest();
    const auto outputFifo = ANKAIOS_CONTROL_INTERFACE_BASE_PATH + "/output";
    std::ofstream output{outputFifo, std::ios::app | std::ios::binary};
    if (output.fail())
    {
        logging::log(std::cerr, "Error: could not open file ", outputFifo);
        return;
    }

    logging::log(std::cout,
                 "Sending StateChangeRequest containing details for adding the dynamic workload \'dynamic_nginx\':\n",
                 "StateChangeRequest {\n",
                 updateWorkloadState.DebugString(),
                 "}\n");
    // write length-delimited protobuf message into output fifo to add the new workload
    google::protobuf::util::SerializeDelimitedToOstream(updateWorkloadState, &output);
    output.flush();

    const auto requestCompleteState = createRequestCompleteStateRequest();
    bool send_result = false;
    do
    {
        logging::log(std::cout,
                     "Sending StateChangeRequest containing details for requesting all workload states:\n",
                     "StateChangeRequest {\n",
                     requestCompleteState.DebugString(),
                     "}\n");
        // write length-delimited protobuf message into output fifo to receive the workload states
        send_result = google::protobuf::util::SerializeDelimitedToOstream(requestCompleteState, &output);
        output.flush();
        std::this_thread::sleep_for(std::chrono::seconds(WAITING_TIME_IN_SEC));
    } while (send_result);
}

int main()
{
    std::thread readThread{readFromControlInterface};
    writeToControlInterface();
    readThread.join();
    return 0;
}
