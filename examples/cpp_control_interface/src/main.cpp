#include <iostream>
#include <fstream>
#include <thread>
#include <chrono>
#include <iomanip>
#include "src/proto/ank_base.pb.h"
#include "src/proto/control_api.pb.h"
#include <google/protobuf/io/coded_stream.h>
#include <google/protobuf/util/delimited_message_util.h>
#include <google/protobuf/io/zero_copy_stream_impl.h>

static const std::string ANKAIOS_CONTROL_INTERFACE_BASE_PATH{"/run/ankaios/control_interface"};
static const int WAITING_TIME_IN_SEC { 5 };
static const char* REQUEST_ID{ "dynamic_nginx@cpp_control_interface" };

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

/* Create the Request containing an UpdateStateRequest
    that contains the details for adding the new workload and
    the update mask to add only the new workload. */
control_api::ToAnkaios createRequestToAddNewWorkload()
{
    ank_base::Workload newWorkload;
    newWorkload.set_agent("agent_A");
    newWorkload.set_runtime("podman");
    newWorkload.set_restartpolicy(ank_base::RestartPolicy::NEVER);
    newWorkload.set_runtimeconfig("image: docker.io/library/nginx\ncommandOptions: [\"-p\", \"8080:80\"]");

    ank_base::State *state{new ank_base::State};
    std::string* apiVersion{new std::string("v0.1")};
    state->set_allocated_apiversion(apiVersion);
    ank_base::WorkloadMap *workloads{new ank_base::WorkloadMap};
    workloads->mutable_workloads()->insert({"dynamic_nginx", std::move(newWorkload)});
    state->set_allocated_workloads(workloads);

    ank_base::CompleteState *completeState{new ank_base::CompleteState};
    completeState->set_allocated_desiredstate(state);

    ank_base::UpdateStateRequest *updateStateRequest{new ank_base::UpdateStateRequest};
    updateStateRequest->set_allocated_newstate(completeState);
    updateStateRequest->add_updatemask("desiredState.workloads.dynamic_nginx");

    ank_base::Request* request {new ank_base::Request};
    request->set_allocated_updatestaterequest(updateStateRequest);
    request->set_requestid(REQUEST_ID);

    control_api::ToAnkaios toAnkaios;
    toAnkaios.set_allocated_request(request);
    return toAnkaios;
}

/* Create a Request to request the CompleteState
    for querying the workload states. */
control_api::ToAnkaios createRequestForCompleteState()
{
    ank_base::CompleteStateRequest* completeStateRequest{new ank_base::CompleteStateRequest};
    completeStateRequest->add_fieldmask("workloadStates.agent_A.dynamic_nginx");

    ank_base::Request* request {new ank_base::Request};
    request->set_allocated_completestaterequest(completeStateRequest);
    request->set_requestid(REQUEST_ID);

    control_api::ToAnkaios toAnkaios;
    toAnkaios.set_allocated_request(request);
    return toAnkaios;
}

/* Reads from the control interface input fifo and prints the workload states. */
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
        control_api::FromAnkaios fromAnkaios;
        bool clean_eof = false;
        // read length-delimited protobuf message to output the workload states
        result = google::protobuf::util::ParseDelimitedFromZeroCopyStream(&fromAnkaios, &bufferedInputStream, &clean_eof);
        if (!result)
        {
            logging::log(std::cerr, "Invalid response, parsing error.");
            continue;
        }

        const auto requestId = fromAnkaios.response().requestid();
        if (requestId == REQUEST_ID)
        {
        logging::log(std::cout,
                        "Receiving Response containing the workload states of the current state:\n",
                        "FromAnkaios {\n",
                        fromAnkaios.DebugString(),
                        "}\n");
        } else
        {
            logging::log(std::cout, "RequestId does not match. Skipping messages from requestId: ", requestId);
        }
    } while (result);
}

/* Writes a Request into the control interface output fifo
    to add the new workload dynamically and every x sec according to WAITING_TIME_IN_SEC
    another Request to request the workload states. */
void writeToControlInterface()
{
    const auto requestToAddNewWorkload = createRequestToAddNewWorkload();
    const auto outputFifo = ANKAIOS_CONTROL_INTERFACE_BASE_PATH + "/output";
    std::ofstream output{outputFifo, std::ios::app | std::ios::binary};
    if (output.fail())
    {
        logging::log(std::cerr, "Error: could not open file ", outputFifo);
        return;
    }

    logging::log(std::cout,
                 "Sending Request containing details for adding the dynamic workload \"dynamic_nginx\":\n",
                 "ToAnkaios {\n",
                 requestToAddNewWorkload.DebugString(),
                 "}\n");
    // write length-delimited protobuf message into output fifo to add the new workload
    google::protobuf::util::SerializeDelimitedToOstream(requestToAddNewWorkload, &output);
    output.flush();

    const auto requestForCompleteState = createRequestForCompleteState();
    bool send_result = false;
    do
    {
        logging::log(std::cout,
                     "Sending Request containing details for requesting all workload states:\n",
                     "ToAnkaios {\n",
                     requestForCompleteState.DebugString(),
                     "}\n");
        // write length-delimited protobuf message into output fifo to receive the workload states
        send_result = google::protobuf::util::SerializeDelimitedToOstream(requestForCompleteState, &output);
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
