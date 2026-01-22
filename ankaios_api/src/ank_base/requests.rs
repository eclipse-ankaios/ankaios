// Copyright (c) 2025 Elektrobit Automotive GmbH
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

use crate::ank_base::Request;

impl Request {
    pub fn prefix_id(prefix: &str, request_id: &String) -> String {
        format!("{prefix}{request_id}")
    }

    pub fn prefix_request_id(&mut self, prefix: &str) {
        self.request_id = Self::prefix_id(prefix, &self.request_id);
    }
}

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use crate::ank_base::{LogsCancelRequest, RequestContent, Request};

    #[test]
    fn test_prefix_request_id() {
        let mut request_spec = Request {
            request_id: "12345".to_string(),
            request_content: RequestContent::LogsCancelRequest(LogsCancelRequest {}),
        };
        request_spec.prefix_request_id("PRE");
        assert_eq!(request_spec.request_id, "PRE12345");
    }
}
