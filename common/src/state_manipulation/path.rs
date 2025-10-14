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

use crate::PATH_SEPARATOR;

#[derive(Debug, Hash, PartialEq, Eq)]
pub struct Path {
    parts: Vec<String>,
}

impl Path {
    pub fn split_last(&self) -> Result<(Path, String), String> {
        let (last, head) = self
            .parts
            .split_last()
            .ok_or_else(|| String::from("The given path is empty"))?;
        Ok((
            Path {
                parts: head.to_owned(),
            },
            last.to_owned(),
        ))
    }

    pub fn parts(&self) -> &Vec<String> {
        &self.parts
    }
}

// [impl->swdd~common-state-manipulation-path~1]
impl From<&str> for Path {
    fn from(value: &str) -> Self {
        Path {
            parts: if value.is_empty() {
                vec![]
            } else {
                value.split(PATH_SEPARATOR).map(|x| x.into()).collect()
            },
        }
    }
}

impl From<String> for Path {
    fn from(value: String) -> Self {
        From::<&str>::from(&value)
    }
}

impl From<&String> for Path {
    fn from(value: &String) -> Self {
        From::<&str>::from(value)
    }
}

impl From<Path> for String {
    fn from(value: Path) -> Self {
        (&value).into()
    }
}

impl From<&Path> for String {
    fn from(value: &Path) -> Self {
        value.parts.join(".")
    }
}

impl From<Vec<String>> for Path {
    fn from(value: Vec<String>) -> Self {
        Path { parts: value }
    }
}

impl std::fmt::Display for Path {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", String::from(self))
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

    // [utest->swdd~common-state-manipulation-path~1]
    #[test]
    fn utest_path_from_string() {
        let path_string: String = "1.2.3".into();

        let expected = Path {
            parts: vec!["1", "2", "3"].into_iter().map(|x| x.into()).collect(),
        };
        let actual: Path = path_string.into();

        assert_eq!(actual, expected)
    }

    #[test]
    fn utest_path_from_empty_string() {
        let path_string: String = "".into();

        let expected = Path { parts: vec![] };
        let actual: Path = path_string.into();

        assert_eq!(actual, expected)
    }

    // [utest->swdd~common-state-manipulation-path~1]
    #[test]
    fn utest_path_from_string_ref() {
        let path_string: String = "1.2.3".into();

        let expected = Path {
            parts: vec!["1", "2", "3"].into_iter().map(|x| x.into()).collect(),
        };
        let actual: Path = (&path_string).into();

        assert_eq!(actual, expected)
    }

    #[test]
    fn utest_path_from_empty_string_ref() {
        let path_string: String = "".into();

        let expected = Path { parts: vec![] };
        let actual: Path = (&path_string).into();

        assert_eq!(actual, expected)
    }

    // [utest->swdd~common-state-manipulation-path~1]
    #[test]
    fn utest_path_from_str() {
        let expected = Path {
            parts: vec!["1", "2", "3"].into_iter().map(|x| x.into()).collect(),
        };
        let actual: Path = "1.2.3".into();

        assert_eq!(actual, expected)
    }

    #[test]
    fn utest_path_from_empty_str() {
        let expected = Path { parts: vec![] };
        let actual: Path = "".into();

        assert_eq!(actual, expected)
    }
}
