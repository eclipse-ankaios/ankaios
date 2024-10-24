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
use serde::{Serialize, Serializer};
use std::collections::{BTreeMap, HashMap};

use crate::{std_extensions::IllegalStateResult, ANKAIOS_VERSION};
use semver::Version;

// [impl->swdd~common-helper-methods~1]
pub fn try_into_vec<S, T, E>(input: Vec<S>) -> Result<Vec<T>, E>
where
    T: TryFrom<S, Error = E>,
{
    input.into_iter().map(|x| x.try_into()).collect()
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

// [impl->swdd~common-version-checking~1]
pub fn check_version_compatibility(version: impl AsRef<str>) -> Result<(), String> {
    let ank_version = Version::parse(ANKAIOS_VERSION).unwrap_or_illegal_state();
    if let Ok(input_version) = Version::parse(version.as_ref()) {
        if ank_version.major == input_version.major &&
        // As we are at a 0 (zero) major version, we also require minor version equality
        ank_version.minor == input_version.minor
        {
            return Ok(());
        }
    } else {
        log::warn!(
            "Could not parse incoming string '{}' as semantic version.",
            version.as_ref()
        );
    }

    Err(format!(
        "Unsupported protocol version '{}'. Currently supported '{ANKAIOS_VERSION}'",
        version.as_ref()
    ))
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
    use semver::Version;

    use crate::{check_version_compatibility, ANKAIOS_VERSION};

    // [utest->swdd~common-version-checking~1]
    #[test]
    fn utest_version_compatibility_success() {
        assert!(check_version_compatibility(ANKAIOS_VERSION).is_ok())
    }

    // [utest->swdd~common-version-checking~1]
    #[test]
    fn utest_version_compatibility_patch_diff_success() {
        let mut version = Version::parse(ANKAIOS_VERSION).unwrap();
        version.patch = 199;
        assert!(check_version_compatibility(version.to_string()).is_ok())
    }

    // [utest->swdd~common-version-checking~1]
    #[test]
    fn utest_version_compatibility_patch_major_error() {
        let mut version = Version::parse(ANKAIOS_VERSION).unwrap();
        version.major = 199;
        assert!(check_version_compatibility(version.to_string()).is_err())
    }

    // [utest->swdd~common-version-checking~1]
    #[test]
    fn utest_version_compatibility_patch_minor_error() {
        let mut version = Version::parse(ANKAIOS_VERSION).unwrap();
        version.minor = 199;
        // Currently we assert that the minor version is also equal as we are at a 0th major version.
        // When a major version is released, we can update the test here and expect an Ok().
        assert_eq!(0, version.major);
        assert!(check_version_compatibility(version.to_string()).is_err())
    }
}
