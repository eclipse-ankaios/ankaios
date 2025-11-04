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

}
