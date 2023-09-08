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

const SEPARATOR: &str = "@";

pub fn prepend_request_id(request_id: &str, agent_name: &str) -> String {
    if request_id.is_empty() {
        return String::from("");
    }
    if agent_name.is_empty() {
        return request_id.to_owned();
    }
    format!("{agent_name}{SEPARATOR}{request_id}")
}

pub fn detach_prefix_from_request_id(request_id: &str) -> (String, String) {
    if request_id.is_empty() {
        return (String::from(""), String::from(""));
    }
    let mut splitted = request_id.splitn(2, SEPARATOR);
    let prefix = splitted.next().unwrap_or(Default::default());
    if let Some(raw_request_id) = splitted.next() {
        (prefix.to_owned(), raw_request_id.to_owned())
    } else {
        (String::from(""), prefix.to_owned()) //prefix is request_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utest_prepend_request_id_returns_empty_when_provided_request_id_is_empty() {
        assert_eq!(String::from(""), prepend_request_id("", "don't care"));
    }
    #[test]
    fn utest_prepend_request_id_returns_request_id_when_provided_agent_name_is_empty() {
        assert_eq!(
            String::from("my_request_id"),
            prepend_request_id("my_request_id", "")
        );
    }
    #[test]
    fn utest_prepend_request_id_returns_with_agent_name_prefixed_request_id_when_provided_request_id_and_agent_name(
    ) {
        assert_eq!(
            String::from("agent_name@my_request_id"),
            prepend_request_id("my_request_id", "agent_name")
        );
    }
    #[test]
    fn utest_detach_prefix_from_request_id_returns_empty_tuple_when_provided_empty_request_id() {
        assert_eq!(
            (String::from(""), String::from("")),
            detach_prefix_from_request_id("")
        );
    }
    #[test]
    fn utest_detach_prefix_from_request_id_returns_a_tuple_with_empty_prefix_and_request_id_when_provided_raw_request_id(
    ) {
        assert_eq!(
            (String::from(""), String::from("my_request_id")),
            detach_prefix_from_request_id("my_request_id")
        );
    }
    #[test]
    fn utest_detach_prefix_from_request_id_returns_a_tuple_prefix_and_request_id_when_provided_prefixed_request_id(
    ) {
        assert_eq!(
            (String::from("prefix"), String::from("my_request_id")),
            detach_prefix_from_request_id("prefix@my_request_id")
        );
    }
    #[test]
    fn utest_detach_prefix_from_request_id_returns_a_tuple_prefix_and_request_id_when_provided_prefixed_request_id_with_multiple_separators(
    ) {
        assert_eq!(
            (
                String::from("prefix"),
                String::from("my_request_id_part1@my_request_id_part2")
            ),
            detach_prefix_from_request_id("prefix@my_request_id_part1@my_request_id_part2")
        );
    }
}
