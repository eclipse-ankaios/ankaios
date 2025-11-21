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

use serde::{Deserialize, Serialize, Serializer};
use std::collections::{BTreeMap, HashMap};

pub fn serialize_option_to_ordered_map<S, T: Serialize>(
    value: &Option<HashMap<String, T>>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    if let Some(value) = value {
        serialize_to_ordered_map(value, serializer)
    } else {
        serializer.serialize_none()
    }
}

pub fn serialize_to_ordered_map<S, T: Serialize>(
    value: &HashMap<String, T>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let ordered: BTreeMap<_, _> = value.iter().collect();
    ordered.serialize(serializer)
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum MapOrVec<K: std::hash::Hash + Eq, V> {
    Map(HashMap<K, V>),
    Vec(Vec<MapEntry<K, V>>),
}

#[derive(Debug, Serialize, Deserialize)]
struct MapEntry<K, V> {
    key: K,
    value: V,
}

pub fn tag_adapter_deserializer<'de, D, K, V>(deserializer: D) -> Result<HashMap<K, V>, D::Error>
where
    D: serde::Deserializer<'de>,
    K: std::hash::Hash + Eq + Deserialize<'de>,
    V: Deserialize<'de>,
{
    let map_or_vec = MapOrVec::<K, V>::deserialize(deserializer)?;
    match map_or_vec {
        MapOrVec::Map(m) => Ok(m),
        MapOrVec::Vec(v) => {
            let mut map = HashMap::new();
            for entry in v {
                map.insert(entry.key, entry.value);
            }
            Ok(map)
        }
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
    use super::*;

    #[test]
    fn utest_serialize_option_to_ordered_map() {
        let mut some_serializer = serde_yaml::Serializer::new(Vec::new());

        let mut map = HashMap::new();
        map.insert("b".to_string(), 2);
        map.insert("a".to_string(), 1);
        let option_map = Some(map);

        let result = serialize_option_to_ordered_map(&option_map, &mut some_serializer);
        assert!(result.is_ok());
        assert_eq!(some_serializer.into_inner().unwrap(), b"a: 1\nb: 2\n");

        let mut none_serializer = serde_yaml::Serializer::new(Vec::new());

        let result = serialize_option_to_ordered_map::<_, i32>(&None, &mut none_serializer);
        assert!(result.is_ok());
        assert_eq!(none_serializer.into_inner().unwrap(), b"null\n");
    }

    #[test]
    fn utest_serialize_to_ordered_map() {
        let mut serializer = serde_yaml::Serializer::new(Vec::new());

        let mut map = HashMap::new();
        map.insert("b".to_string(), 2);
        map.insert("a".to_string(), 1);
        map.insert("c".to_string(), 3);

        let result = serialize_to_ordered_map(&map, &mut serializer);
        assert!(result.is_ok());
        assert_eq!(serializer.into_inner().unwrap(), b"a: 1\nb: 2\nc: 3\n");
    }

    #[test]
    fn utest_tag_adapter_deserializer() {
        let yaml_map = r#"
            key1: value1
            key2: value2
        "#;

        let deserialized_map: HashMap<String, String> = serde_yaml::from_str(yaml_map).unwrap();
        let adapted_map =
            tag_adapter_deserializer(serde_yaml::Deserializer::from_str(yaml_map)).unwrap();
        assert_eq!(deserialized_map, adapted_map);

        let yaml_vec = r#"
            - key: key1
              value: value1
            - key: key2
              value: value2
        "#;

        let adapted_map_vec =
            tag_adapter_deserializer(serde_yaml::Deserializer::from_str(yaml_vec)).unwrap();
        assert_eq!(deserialized_map, adapted_map_vec);
    }
}
