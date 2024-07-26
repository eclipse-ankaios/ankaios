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

pub mod ank_base {

    // [impl->swdd~ank-base-provides-object-definitions~1]
    tonic::include_proto!("ank_base"); // The string specified here must match the proto package name

    impl Response {
        pub fn access_denied(request_id: String) -> Response {
            Response {
                request_id,
                response_content: response::ResponseContent::Error(Error {
                    message: "Access denied".into(),
                })
                .into(),
            }
        }
    }
}

pub mod control_api {
    // [impl->swdd~control-api-provides-control-interface-definitions~1]
    tonic::include_proto!("control_api"); // The string specified here must match the proto package name
}
