// Copyright (c) 2024 Elektrobit Automotive GmbH
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
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct File {
    pub mount_point: String,
    #[serde(flatten)]
    pub file_content: FileContent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum FileContent {
    Data(Data),
    BinaryData(BinaryData),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Data {
    data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BinaryData {
    binary_data: String,
}

impl TryFrom<ank_base::File> for File {
    type Error = String;

    fn try_from(value: ank_base::File) -> Result<Self, String> {
        Ok(File {
            mount_point: value.mount_point,
            file_content: match value.file_content {
                Some(ank_base::file::FileContent::Data(data)) => FileContent::Data(Data { data }),
                Some(ank_base::file::FileContent::BinaryData(data)) => {
                    FileContent::BinaryData(BinaryData { binary_data: data })
                }
                None => return Err("Missing field 'fileContent'".to_string()),
            },
        })
    }
}

impl From<File> for ank_base::File {
    fn from(item: File) -> Self {
        ank_base::File {
            mount_point: item.mount_point,
            file_content: match item.file_content {
                FileContent::Data(data) => Some(ank_base::file::FileContent::Data(data.data)),
                FileContent::BinaryData(data) => {
                    Some(ank_base::file::FileContent::BinaryData(data.binary_data))
                }
            },
        }
    }
}
