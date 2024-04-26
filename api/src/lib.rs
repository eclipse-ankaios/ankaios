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

pub mod ank_proto {

    // [impl->swdd~grpc-delegate-workflow-to-external-library~1]
    tonic::include_proto!("ank_proto"); // The string specified here must match the proto package name
}

pub mod control_interface_api {
    // TODO: trace reqs for the control interface
    tonic::include_proto!("control_interface_api"); // The string specified here must match the proto package name
}

pub mod grpc_api {

    // [impl->swdd~grpc-delegate-workflow-to-external-library~1]
    tonic::include_proto!("grpc_api"); // The string specified here must match the proto package name
}
