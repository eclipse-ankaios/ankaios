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
use api::ank_base;

pub type File = ank_base::FileInternal;
pub type FileContent = ank_base::file::FileContentInternal;

// #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
// #[serde(rename_all = "camelCase")]
// pub struct File {
//     pub mount_point: String,
//     #[serde(flatten)]
//     pub file_content: FileContent,
// }

// #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
// #[serde(untagged)]
// pub enum FileContent {
//     Data(Data),
//     BinaryData(Base64Data),
// }

// #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
// #[serde(rename_all = "camelCase")]
// pub struct Data {
//     pub data: String,
// }

// #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
// #[serde(rename_all = "camelCase")]
// pub struct Base64Data {
//     #[serde(rename = "binaryData")]
//     pub base64_data: String,
// }

// impl TryFrom<ank_base::File> for File {
//     type Error = String;

//     fn try_from(value: ank_base::File) -> Result<Self, String> {
//         Ok(File {
//             mount_point: value.mount_point,
//             file_content: match value.file_content {
//                 Some(ank_base::file::FileContent::Data(data)) => FileContent::Data(Data { data }),
//                 Some(ank_base::file::FileContent::BinaryData(binary_data)) => {
//                     FileContent::BinaryData(Base64Data {
//                         base64_data: binary_data,
//                     })
//                 }
//                 None => return Err("Missing field 'fileContent'".to_string()),
//             },
//         })
//     }
// }

// impl From<File> for ank_base::File {
//     fn from(item: File) -> Self {
//         ank_base::File {
//             mount_point: item.mount_point,
//             file_content: match item.file_content {
//                 FileContent::Data(data) => Some(ank_base::file::FileContent::Data(data.data)),
//                 FileContent::BinaryData(data) => {
//                     Some(ank_base::file::FileContent::BinaryData(data.base64_data))
//                 }
//             },
//         }
//     }
// }

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_rendered_workload_files() -> Vec<File> {
    vec![
        File {
            mount_point: "/file.json".to_string(),
            file_content: FileContent::Data {
                data: "text data".into(),
            },
        },
        File {
            mount_point: "/binary_file".to_string(),
            file_content: FileContent::BinaryData {
                binary_data: "base64_data".into(),
            },
        },
    ]
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
    use super::{File, FileContent};
    use api::ank_base;

    const MOUNT_POINT_1: &str = "/file.json";
    const MOUNT_POINT_2: &str = "/binary_file";
    const TEXT_FILE_CONTENT: &str = "text data";
    const BASE64_FILE_CONTENT: &str = "base64_data";

    #[test]
    fn utest_convert_text_file_to_ankaios() {
        let proto_file = ank_base::File {
            mount_point: MOUNT_POINT_1.to_owned(),
            file_content: Some(ank_base::file::FileContent::Data(
                TEXT_FILE_CONTENT.to_owned(),
            )),
        };

        let result = File::try_from(proto_file);

        assert!(result.is_ok());
        let ankaios_file = result.unwrap();
        assert_eq!(ankaios_file.mount_point, MOUNT_POINT_1);
        assert_eq!(
            ankaios_file.file_content,
            FileContent::Data {
                data: TEXT_FILE_CONTENT.to_owned()
            }
        );
    }

    #[test]
    fn utest_convert_binary_file_to_ankaios() {
        let proto_binary_file = ank_base::File {
            mount_point: MOUNT_POINT_2.to_owned(),
            file_content: Some(ank_base::file::FileContent::BinaryData(
                BASE64_FILE_CONTENT.to_owned(),
            )),
        };

        let result = File::try_from(proto_binary_file);

        assert!(result.is_ok());
        let ankaios_file = result.unwrap();
        assert_eq!(ankaios_file.mount_point, MOUNT_POINT_2);
        assert_eq!(
            ankaios_file.file_content,
            FileContent::BinaryData {
                binary_data: BASE64_FILE_CONTENT.to_owned()
            }
        );
    }

    #[test]
    fn utest_convert_proto_file_to_ankaios_no_file_content_fails() {
        let proto_binary_file = ank_base::File {
            mount_point: MOUNT_POINT_2.to_owned(),
            file_content: None,
        };

        let result = File::try_from(proto_binary_file);

        assert_eq!(result, Err("Missing field 'file_content'".to_string()));
    }

    #[test]
    fn utest_convert_text_file_to_proto() {
        let text_file = File {
            mount_point: MOUNT_POINT_1.to_owned(),
            file_content: FileContent::Data {
                data: TEXT_FILE_CONTENT.to_owned(),
            },
        };

        let file_as_proto = ank_base::File::from(text_file);

        assert_eq!(file_as_proto.mount_point, MOUNT_POINT_1);
        assert!(file_as_proto.file_content.is_some());
        let proto_text_file_content = file_as_proto.file_content.unwrap();
        assert_eq!(
            proto_text_file_content,
            ank_base::file::FileContent::Data(TEXT_FILE_CONTENT.to_owned())
        );
    }

    #[test]
    fn utest_convert_binary_file_to_proto() {
        let binary_file = File {
            mount_point: MOUNT_POINT_2.to_owned(),
            file_content: FileContent::BinaryData {
                binary_data: BASE64_FILE_CONTENT.to_owned(),
            },
        };

        let binary_file_as_proto = ank_base::File::from(binary_file);

        assert_eq!(binary_file_as_proto.mount_point, MOUNT_POINT_2);
        assert!(binary_file_as_proto.file_content.is_some());
        let proto_binary_file_content = binary_file_as_proto.file_content.unwrap();
        assert_eq!(
            proto_binary_file_content,
            ank_base::file::FileContent::BinaryData(BASE64_FILE_CONTENT.to_owned())
        );
    }
}
