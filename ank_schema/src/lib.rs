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

use ankaios_api::{PREVIOUS_API_VERSION, ank_base::StateSpec};
use schemars::generate::SchemaSettings;

fn get_schema_value() -> Result<serde_json::Value, String> {
    let generator = SchemaSettings::draft07().into_generator();
    let schema = generator.into_root_schema_for::<StateSpec>();
    serde_json::to_value(&schema).map_err(|e| format!("Failed to serialize schema: {e}"))
}

// [impl->swdd~cli-validates-manifest-against-schema~1]
pub fn validate_manifest(instance: &serde_json::Value) -> Result<(), String> {
    // The deprecated API version uses a different structure (e.g. tags as a sequence)
    // that is not described by the current schema, so skip schema validation for it.
    let api_version = instance
        .get("apiVersion")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if api_version == PREVIOUS_API_VERSION {
        return Ok(());
    }

    let schema_value = get_schema_value()?;

    let validator = jsonschema::options()
        .with_draft(jsonschema::Draft::Draft7)
        .build(&schema_value)
        .map_err(|e| format!("Failed to build schema validator: {e}"))?;

    let errors: Vec<String> = validator
        .iter_errors(instance)
        .map(|e| format!("{e} at '{}'", e.instance_path()))
        .collect();

    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Manifest schema validation failed:\n{}",
            errors.join("\n")
        ))
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
    use serde_json::json;

    #[test]
    fn utest_validate_manifest_skips_deprecated_v01() {
        let manifest = json!({
            "apiVersion": "v0.1",
            "workloads": {
                "nginx": {
                    "agent": "agent_A",
                    "runtime": "podman",
                    "runtimeConfig": "image: nginx:latest"
                }
            }
        });
        assert!(validate_manifest(&manifest).is_ok());
    }

    #[test]
    fn utest_validate_manifest_missing_api_version_fails() {
        let manifest = json!({
            "workloads": {}
        });
        let result = validate_manifest(&manifest);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("Manifest schema validation failed")
        );
    }

    #[test]
    fn utest_validate_manifest_valid_v1() {
        let manifest = json!({
            "apiVersion": "v1",
            "workloads": {
                "nginx": {
                    "agent": "agent_A",
                    "runtime": "podman",
                    "runtimeConfig": "image: nginx:latest"
                }
            }
        });
        assert!(validate_manifest(&manifest).is_ok());
    }

    #[test]
    fn utest_validate_manifest_invalid_api_version_pattern() {
        let manifest = json!({
            "apiVersion": "v2",
            "workloads": {}
        });
        let result = validate_manifest(&manifest);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("Manifest schema validation failed")
        );
    }

    #[test]
    fn utest_validate_manifest_invalid_workload_name() {
        let manifest = json!({
            "apiVersion": "v1",
            "workloads": {
                "invalid.workload.name": {
                    "agent": "agent_A",
                    "runtime": "podman",
                    "runtimeConfig": "image: nginx:latest"
                }
            }
        });
        let result = validate_manifest(&manifest);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("Manifest schema validation failed")
        );
    }
}
