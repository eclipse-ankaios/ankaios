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

use std::fmt::Display;
use common::PATH_SEPARATOR;

#[derive(Clone, Debug)]
pub struct Path {
    pub sections: Vec<String>,
}

impl From<&str> for Path {
    fn from(value: &str) -> Self {
        Self {
            sections: if value.is_empty() {
                Vec::new()
            } else {
                value.split(PATH_SEPARATOR).map(Into::into).collect()
            },
        }
    }
}

impl Display for Path {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.sections.join("."))
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
    use super::Path;
    use common::PATH_SEPARATOR;

    #[test]
    fn utest_from_empty_str() {
        let path = Path::from("");

        assert!(path.sections.is_empty())
    }

    #[test]
    fn utest_from_str() {
        let path = Path::from(format!("abc{PATH_SEPARATOR}def{PATH_SEPARATOR}ghi").as_str());

        assert_eq!(path.sections, ["abc", "def", "ghi"]);
    }

    #[test]
    fn utest_to_str() {
        let path = Path {
            sections: vec!["abc".into(), "def".into(), "ghi".into()],
        };

        assert_eq!(path.to_string(), "abc.def.ghi");
    }
}
