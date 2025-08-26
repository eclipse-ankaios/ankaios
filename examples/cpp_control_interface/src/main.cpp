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

// ======================================================================
// Imports
// ======================================================================
// - ank_base.pb.h and control_api.pb.h: Ankaios protocol definitions
// - google/protobuf/*: Used for encoding and decoding protobuf messages.
#include <iostream>
#include <fstream>
#include <thread>
#include <atomic>
#include <optional>
#include <chrono>
#include <iomanip>
#include <sys/stat.h>
#include "src/proto/ank_base.pb.h"
#include "src/proto/control_api.pb.h"
#include <google/protobuf/io/coded_stream.h>
#include <google/protobuf/util/delimited_message_util.h>
#include <google/protobuf/io/zero_copy_stream_impl.h>


// =======================================================================
// Variables - general
// =======================================================================
// - ANKAIOS_CONTROL_INTERFACE_BASE_PATH is the path to the input and output
//     FIFO files of the control interface.
// - WAITING_TIME_IN_SEC represents the default waiting time.
// - UPDATE_STATE_REQUEST_ID is the request ID for the update state request.
// - COMPLETE_STATE_REQUEST_ID is the request ID for the complete state request.
// - PROTOCOL_VERSION is the version of Ankaios, which for this example
//     is set to the value of the environment variable 'ANKAIOS_VERSION'.
// - CONNECTED is a flag to check the connection is established.
// - CONNECTION_CLOSED is a flag to check if Ankaios closed the connection.
static const std::string ANKAIOS_CONTROL_INTERFACE_BASE_PATH{"/run/ankaios/control_interface"};
static const int WAITING_TIME_IN_SEC { 5 };
static const char* UPDATE_STATE_REQUEST_ID{ "dynamic_nginx@12345" };
static const char* COMPLETE_STATE_REQUEST_ID{ "dynamic_nginx@67890" };
static const char* PROTOCOL_VERSION{ std::getenv("ANKAIOS_VERSION") ? std::getenv("ANKAIOS_VERSION") : "v0.1" };
std::atomic<bool> CONNECTED{false};
std::atomic<bool> CONNECTION_CLOSED{false};

// =======================================================================
// Setup logger & utility functions
// =======================================================================
namespace logging {
    /* Log function that logs various arguments
        to the specified stream in a custom log format. */
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
    struct stat buffer;
    return (stat(path.c_str(), &buffer) == 0);
}

// =======================================================================
// Functions for creating protobuf messages
// =======================================================================
// - CreateHelloMessage returns the initial required message to establish
//     a connection with Ankaios.
// - CreateRequestToAddNewWorkload returns the message used to update
//     the state of the cluster. In this example, it is used to add a new
//     workload dynamically. It contains the details for adding the new
//     workload and the update mask to add only the new workload.
// - CreateRequestForCompleteState returns a request for querying the
//     state of the dynamic_nginx workload.
control_api::ToAnkaios CreateHelloMessage() {
    control_api::Hello* hello{new control_api::Hello};
    hello->set_protocolversion(PROTOCOL_VERSION);

    control_api::ToAnkaios to_ankaios;
    to_ankaios.set_allocated_hello(hello);
    return to_ankaios;
}

control_api::ToAnkaios CreateRequestToAddNewWorkload() {
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

control_api::ToAnkaios CreateRequestForCompleteState() {
    ank_base::CompleteStateRequest* complete_state_request{new ank_base::CompleteStateRequest};
    complete_state_request->add_fieldmask("workloadStates.agent_A.dynamic_nginx");

    ank_base::Request* request{new ank_base::Request};
    request->set_allocated_completestaterequest(complete_state_request);
    request->set_requestid(COMPLETE_STATE_REQUEST_ID);

    control_api::ToAnkaios to_ankaios;
    to_ankaios.set_allocated_request(request);
    return to_ankaios;
}

// =======================================================================
// Ankaios control interface methods
// =======================================================================
// - ReadProtobufData reads the protobuf message from the control interface
//     input fifo.
// - HandleResponse processes the response from Ankaios. It checks the type
//     of the response and handles it accordingly.
// - ReadFromControlInterface continuously reads from the control interface
//     input fifo and sends the response to be handled.
// - WriteToControlInterface writes a ToAnkaios message to the control
//     interface output fifo.
std::optional<control_api::FromAnkaios> ReadProtobufData(google::protobuf::io::IstreamInputStream* stream) {
    control_api::FromAnkaios message;
    bool clean_eof = false;

    if (!google::protobuf::util::ParseDelimitedFromZeroCopyStream(&message, stream, &clean_eof)) {
        logging::Log(std::cerr, "Invalid response, parsing error.");
        return std::nullopt;
    }
    return message;
}

void HandleResponse(const control_api::FromAnkaios& from_ankaios) {
    // Check if the connection has been established or not
    if (!CONNECTED.load()) {
        if (from_ankaios.has_controlinterfaceaccepted()) {
            logging::Log(std::cout, "Received Control interface accepted response.");
            CONNECTED.store(true);
        }
        else if (from_ankaios.has_connectionclosed()) {
            logging::Log(std::cout, "Received Connection Closed response. Exiting..");
            CONNECTION_CLOSED.store(true);
        }
        else {
            logging::Log(std::cout, "Received unexpected response before connection established. Skipping.");
        }
    }
    // If the connection is established, handle the response accordingly
    else {
        if (from_ankaios.has_response()) {
            const auto request_id = from_ankaios.response().requestid();
            if (request_id == UPDATE_STATE_REQUEST_ID) {
                // Join function to convert repeated field to a comma-separated string
                auto join = [](const auto& rep) {
                    std::ostringstream os;
                    bool first = true;
                    for (const auto& s : rep) {
                        if (!first) os << ", ";
                        os << s;
                        first = false;
                    }
                    return os.str();
                };

                auto added_workloads = join(from_ankaios.response().updatestatesuccess().addedworkloads());
                auto deleted_workloads = join(from_ankaios.response().updatestatesuccess().deletedworkloads());
                logging::Log(std::cout, "Receiving Response for the UpdateStateRequest:\nadded workloads: ", added_workloads, "\ndeleted workloads: ", deleted_workloads);
            }
            else if (request_id == COMPLETE_STATE_REQUEST_ID) {
                logging::Log(std::cout,
                    "Receiving Response for the CompleteStateRequest:\n",
                    from_ankaios.DebugString()
                );
            }
            else {
                logging::Log(std::cout, "RequestId does not match. Skipping messages from requestId: ", request_id);
            }
        }
        else if (from_ankaios.has_connectionclosed()) {
            logging::Log(std::cout, "Received Connection Closed response. Exiting..");
            CONNECTION_CLOSED.store(true);
            CONNECTED.store(false);
        }
        else {
            logging::Log(std::cout, "Received unknown message type. Skipping message.");
        }
    }
}

void ReadFromControlInterface(std::string input_fifo) {
    // Open the input fifo for reading
    std::ifstream input_stream{input_fifo, std::ios::in | std::ios::binary};

    if (input_stream.fail()) {
        logging::Log(std::cerr, "Error: could not open input fifo.");
        return;
    }

    const int BLOCK_SIZE = 1; // disable blocking I/O by setting block_size to 1 byte instead of default value
    google::protobuf::io::IstreamInputStream buffered_stream{&input_stream, BLOCK_SIZE};

    while (!CONNECTION_CLOSED.load()) {
        if (auto from_ankaios = ReadProtobufData(&buffered_stream)) {
            HandleResponse(*from_ankaios);
        } else {
            logging::Log(std::cerr, "Error: Invalid response, parsing error.");
            continue;
        }
    };
}


void WriteToControlInterface(std::ofstream& output_stream, const control_api::ToAnkaios& message) {
    // write length-delimited protobuf message into output fifo
    google::protobuf::util::SerializeDelimitedToOstream(message, &output_stream);
    output_stream.flush();
}

// =======================================================================
// Main
// =======================================================================
int main() {
    // Check if the control interface fifo files exist
    const auto input_fifo = ANKAIOS_CONTROL_INTERFACE_BASE_PATH + "/input";
    const auto output_fifo = ANKAIOS_CONTROL_INTERFACE_BASE_PATH + "/output";
    if (!FileExists(input_fifo) || !FileExists(output_fifo)) {
        logging::Log(std::cerr, "Error: Control interface FIFO files do not exist. Exiting..");
        return 1;
    }

    // Start the reading thread
    std::thread read_thread{ReadFromControlInterface, input_fifo};

    // Open file for writing to the control interface
    std::ofstream output_stream{output_fifo, std::ios::app | std::ios::binary};
    if (output_stream.fail()) {
        logging::Log(std::cerr, "Error: could not open file ", output_fifo);
        return 2;
    }

    // Send hello message to establish the connection
    const auto hello_message = CreateHelloMessage();
    logging::Log(std::cout, "Sending initial Hello message to establish connection...");
    WriteToControlInterface(output_stream, hello_message);
    std::this_thread::sleep_for(std::chrono::seconds(1)); // Give some time for the connection to be established
    if (!CONNECTED.load()) {
        logging::Log(std::cerr, "Connection to Ankaios not established.");
        return 3;
    }

    // Send the request to add the dynamic_nginx workload
    const auto request_to_add_workload = CreateRequestToAddNewWorkload();
    logging::Log(std::cout, "Requesting to add the dynamic_nginx workload...");
    WriteToControlInterface(output_stream, request_to_add_workload);
    std::this_thread::sleep_for(std::chrono::seconds(1));

    while (CONNECTED.load()) {
        // Send the request for the complete state
        const auto request_for_complete_state = CreateRequestForCompleteState();
        logging::Log(std::cout, "Requesting complete state of the dynamic_nginx workload...");
        WriteToControlInterface(output_stream, request_for_complete_state);
        std::this_thread::sleep_for(std::chrono::seconds(WAITING_TIME_IN_SEC));
    }

    // Wait for the reading thread to finish
    read_thread.join();

    return 0;
}
