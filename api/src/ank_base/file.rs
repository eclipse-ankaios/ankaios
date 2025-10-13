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

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_rendered_workload_files() -> Vec<crate::ank_base::FileInternal> {
    vec![
        crate::ank_base::FileInternal {
            mount_point: "/file.json".to_string(),
            file_content: crate::ank_base::FileContentInternal::Data {
                data: "text data".into(),
            },
        },
        crate::ank_base::FileInternal {
            mount_point: "/binary_file".to_string(),
            file_content: crate::ank_base::FileContentInternal::BinaryData {
                binary_data: "base64_data".into(),
            },
        },
    ]
}

#[cfg(test)]
mod tests {
    use crate::ank_base::{File, FileInternal};

    const MOUNT_POINT: &str = "/file";

    #[test]
    fn utest_convert_proto_file_to_ankaios_no_file_content_fails() {
        let proto_binary_file = File {
            mount_point: MOUNT_POINT.to_owned(),
            file_content: None,
        };

        let result = FileInternal::try_from(proto_binary_file);

        assert_eq!(result, Err("Missing field 'file_content'".to_string()));
    }
}
