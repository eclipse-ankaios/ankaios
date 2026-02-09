// Copyright (c) 2023 Elektrobit Automotive GmbH
//
// This program and the accompanying materials are made available under the
// terms of the Apache License, Version 2.0 which is available at
// https://www.apache.org/licenses/LICENSE-2.0.
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
// WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
// License for the specific language governing permissions and limitations
// under the License.
//
// SPDX-License-Identifier: Apache-2.0

#include <iostream>
#include <fstream>
#include <thread>
#include <assert.h>
#include <optional>
#include <chrono>
#include <iomanip>
#include <sys/stat.h>
#include "src/proto/ank_base.pb.h"
#include "src/proto/control_api.pb.h"
#include <google/protobuf/io/coded_stream.h>
#include <google/protobuf/util/delimited_message_util.h>
#include <google/protobuf/io/zero_copy_stream_impl.h>


static const std::string ANKAIOS_CONTROL_INTERFACE_BASE_PATH{"/run/ankaios/control_interface"};
static const int WAITING_TIME_IN_SEC { 5 };
static const char* UPDATE_STATE_REQUEST_ID{ "RWNsaXBzZSBBbmthaW9z" };
static const char* COMPLETE_STATE_REQUEST_ID{ "QW5rYWlvcyBpcyB0aGUgYmVzdA==" };
static const char* PROTOCOL_VERSION{ std::getenv("ANKAIOS_VERSION") ? std::getenv("ANKAIOS_VERSION") : "v0.1" };


namespace logging {
    /* Log function that logs various arguments
     * to the specified stream in a custom log format.
     */
    template <typename... Msgs>
    void Log(std::ostream &stream, Msgs &&...msgs) {
        std::stringstream message;
        ((message << msgs), ...);
        const auto current_time = std::chrono::system_clock::to_time_t(std::chrono::system_clock::now());
        stream << '[' << std::put_time(std::localtime(&current_time), "%FT%TZ") << "] ";
        stream << message.str() << std::endl;
    }
}

bool FileExists(const std::string& path) {
    /* Used to check if a file exists */
    struct stat buffer;
    return (stat(path.c_str(), &buffer) == 0);
}

template <typename Range>
std::string join(const Range& rep) {
    /* Used to return a string representation of an array of strings */
    std::ostringstream os;
    bool first = true;
    for (const auto& s : rep) {
        if (!first) os << ", ";
        os << s;
        first = false;
    }
    return os.str();
}


control_api::ToAnkaios CreateHelloMessage() {
    /* Create hello message for connection */
    control_api::Hello* hello{new control_api::Hello};
    hello->set_protocolversion(PROTOCOL_VERSION);

    control_api::ToAnkaios to_ankaios;
    to_ankaios.set_allocated_hello(hello);
    return to_ankaios;
}

control_api::ToAnkaios CreateRequestToAddNewWorkload() {
    /* Return request for adding a new workload. */
    ank_base::Workload new_workload;
    new_workload.set_agent("agent_A");
    new_workload.set_runtime("podman");
    new_workload.set_restartpolicy(ank_base::RestartPolicy::NEVER);
    new_workload.set_runtimeconfig("image: docker.io/library/nginx\ncommandOptions: [\"-p\", \"8080:80\"]");

    ank_base::State *state{new ank_base::State};
    std::string* api_version{new std::string("v0.1")};
    state->set_allocated_apiversion(api_version);
    ank_base::WorkloadMap *workloads{new ank_base::WorkloadMap};
    workloads->mutable_workloads()->insert({"dynamic_nginx", std::move(new_workload)});
    state->set_allocated_workloads(workloads);

    ank_base::CompleteState *complete_state{new ank_base::CompleteState};
    complete_state->set_allocated_desiredstate(state);

    ank_base::UpdateStateRequest *update_state_request{new ank_base::UpdateStateRequest};
    update_state_request->set_allocated_newstate(complete_state);
    update_state_request->add_updatemask("desiredState.workloads.dynamic_nginx");

    ank_base::Request* request{new ank_base::Request};
    request->set_allocated_updatestaterequest(update_state_request);
    request->set_requestid(UPDATE_STATE_REQUEST_ID);

    control_api::ToAnkaios to_ankaios;
    to_ankaios.set_allocated_request(request);
    return to_ankaios;
}

control_api::ToAnkaios CreateRequestForWorkloadState() {
    /* Return request for getting the complete state */
    ank_base::CompleteStateRequest* complete_state_request{new ank_base::CompleteStateRequest};
    complete_state_request->add_fieldmask("workloadStates.agent_A.dynamic_nginx");

    ank_base::Request* request{new ank_base::Request};
    request->set_allocated_completestaterequest(complete_state_request);
    request->set_requestid(COMPLETE_STATE_REQUEST_ID);

    control_api::ToAnkaios to_ankaios;
    to_ankaios.set_allocated_request(request);
    return to_ankaios;
}


std::optional<control_api::FromAnkaios> ReadFromControlInterface(google::protobuf::io::IstreamInputStream* stream) {
    /* Reads from the control interface input fifo and returns the response */
    control_api::FromAnkaios message;
    bool clean_eof = false;

    if (!google::protobuf::util::ParseDelimitedFromZeroCopyStream(&message, stream, &clean_eof)) {
        logging::Log(std::cerr, "Invalid response, parsing error.");
        return std::nullopt;
    }
    return message;
}


void WriteToControlInterface(std::ofstream& output_stream, const control_api::ToAnkaios& message) {
    // write length-delimited protobuf message into output fifo
    google::protobuf::util::SerializeDelimitedToOstream(message, &output_stream);
    output_stream.flush();
}


int main() {
    // Check if the control interface fifo files exist
    const auto input_fifo = ANKAIOS_CONTROL_INTERFACE_BASE_PATH + "/input";
    const auto output_fifo = ANKAIOS_CONTROL_INTERFACE_BASE_PATH + "/output";
    if (!FileExists(input_fifo) || !FileExists(output_fifo)) {
        logging::Log(std::cerr, "Error: Control interface FIFO files do not exist. Exiting..");
        return 1;
    }

    // Open file for writing to the control interface
    std::ofstream output_stream{output_fifo, std::ios::app | std::ios::binary};
    if (output_stream.fail()) {
        logging::Log(std::cerr, "Error: could not open output fifo.");
        return 2;
    }

    // Send hello message to establish the connection
    const auto hello_message = CreateHelloMessage();
    logging::Log(std::cout, "Sending initial Hello message to establish connection...");
    WriteToControlInterface(output_stream, hello_message);

    // Open the input fifo for reading
    std::ifstream input_stream{input_fifo, std::ios::in | std::ios::binary};
    if (input_stream.fail()) {
        logging::Log(std::cerr, "Error: could not open input fifo.");
        return 3;
    }

    const int BLOCK_SIZE = 1; // disable blocking I/O by setting block_size to 1 byte instead of default value
    google::protobuf::io::IstreamInputStream buffered_stream{&input_stream, BLOCK_SIZE};

    // Check for the response of the hello message
    auto response = ReadFromControlInterface(&buffered_stream);
    assert(response);
    assert(response->has_controlinterfaceaccepted());
    logging::Log(std::cout, "Receiving answer to the initial Hello:\n",
        response->DebugString());

    // Send the request to add the dynamic_nginx workload
    const auto request_to_add_workload = CreateRequestToAddNewWorkload();
    logging::Log(std::cout, "Requesting to add the dynamic_nginx workload...");
    WriteToControlInterface(output_stream, request_to_add_workload);
    response = ReadFromControlInterface(&buffered_stream);
    assert(response);
    assert(response->has_response());
    assert(response->response().has_updatestatesuccess());
    logging::Log(std::cout, "Receiving response for the UpdateStateRequest:\n",
        response->DebugString());

    while (input_stream.is_open() && output_stream.is_open()) {
        // Send the request for the complete state
        const auto request_for_complete_state = CreateRequestForWorkloadState();
        logging::Log(std::cout, "Requesting workload state of the dynamic_nginx workload...");
        WriteToControlInterface(output_stream, request_for_complete_state);
        response = ReadFromControlInterface(&buffered_stream);
        assert(response);
        assert(response->has_response());
        assert(response->response().has_completestateresponse());
        logging::Log(std::cout,
            "Receiving response for the CompleteStateRequest with filter 'workloadStates.agent_A.dynamic_nginx':\n",
            response->DebugString()
        );
        std::this_thread::sleep_for(std::chrono::seconds(WAITING_TIME_IN_SEC));
    }

    output_stream.close();
    input_stream.close();

    return 0;
}
