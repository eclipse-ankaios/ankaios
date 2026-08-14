// Copyright (c) 2026 Elektrobit Automotive GmbH
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
use serde_json::{Map, Value, json};

// [impl->swdd~ank-schema-provides-schema~1]
fn get_schema_value() -> Result<serde_json::Value, String> {
    let generator = SchemaSettings::draft07().into_generator();
    let schema = generator.into_root_schema_for::<StateSpec>();
    let mut value =
        serde_json::to_value(&schema).map_err(|e| format!("Failed to serialize schema: {e}"))?;
    // [impl->swdd~ank-schema-scopes-validation-errors~1]
    simplify_schema(&mut value);
    Ok(value)
}

// [impl->swdd~ank-schema-scopes-validation-errors~1]
// Rewrites `oneOf`/`anyOf` constructs whose default error reporting is
// misleading into semantically equivalent, branch-scoped ones.
fn simplify_schema(value: &mut Value) {
    match value {
        Value::Object(object) => {
            for child in object.values_mut() {
                simplify_schema(child);
            }
            rewrite_nullable(object);
            rewrite_discriminated_one_of(object);
        }
        Value::Array(array) => {
            for element in array {
                simplify_schema(element);
            }
        }
        _ => {}
    }
}

// [impl->swdd~ank-schema-scopes-validation-errors~1]
// Turns a nullable `anyOf`/`oneOf` (from `Option<T>`) into `if`/`then`/`else` so
// a failing concrete branch no longer reports "is not of type null".
fn rewrite_nullable(object: &mut Map<String, Value>) {
    for keyword in ["anyOf", "oneOf"] {
        let Some(branches) = object.get(keyword).and_then(Value::as_array) else {
            continue;
        };

        let null_branch = json!({ "type": "null" });
        if !branches.contains(&null_branch) {
            continue;
        }

        let non_null: Vec<Value> = branches
            .iter()
            .filter(|branch| **branch != null_branch)
            .cloned()
            .collect();

        if non_null.is_empty() {
            continue;
        }

        let else_schema = if non_null.len() == 1 {
            non_null.into_iter().next().unwrap_or(Value::Null)
        } else {
            json!({ keyword: non_null })
        };

        object.remove(keyword);
        object.insert("if".to_owned(), json!({ "type": "null" }));
        object.insert("then".to_owned(), Value::Bool(true));
        object.insert("else".to_owned(), else_schema);
        return;
    }
}

// [impl->swdd~ank-schema-scopes-validation-errors~1]
// Turns an internally tagged enum `oneOf` into a discriminator check plus one
// `if`/`then` branch per variant, so only the matching variant reports errors.
fn rewrite_discriminated_one_of(object: &mut Map<String, Value>) {
    let Some(branches) = object.get("oneOf").and_then(Value::as_array) else {
        return;
    };

    let Some(discriminator) = discriminator_key(branches) else {
        return;
    };

    let branches = branches.clone();
    let allowed_values: Vec<Value> = branches
        .iter()
        .filter_map(|branch| branch["properties"][&discriminator].get("const").cloned())
        .collect();

    let conditional_branches: Vec<Value> = branches
        .iter()
        .map(|branch| {
            let discriminator_value = branch["properties"][&discriminator]["const"].clone();
            json!({
                "if": {
                    "required": [discriminator],
                    "properties": { &discriminator: { "const": discriminator_value } }
                },
                "then": branch
            })
        })
        .collect();

    object.remove("oneOf");
    object.insert("type".to_owned(), json!("object"));

    let required = object
        .entry("required")
        .or_insert_with(|| Value::Array(Vec::new()));
    if let Some(required) = required.as_array_mut() {
        let discriminator_value = Value::String(discriminator.clone());
        if !required.contains(&discriminator_value) {
            required.push(discriminator_value);
        }
    }

    let properties = object
        .entry("properties")
        .or_insert_with(|| Value::Object(Map::new()));
    if let Some(properties) = properties.as_object_mut() {
        properties.insert(discriminator.clone(), json!({ "enum": allowed_values }));
    }

    match object.get_mut("allOf").and_then(Value::as_array_mut) {
        Some(existing) => existing.extend(conditional_branches),
        None => {
            object.insert("allOf".to_owned(), Value::Array(conditional_branches));
        }
    }
}

// [impl->swdd~ank-schema-scopes-validation-errors~1]
// A discriminator is a property pinned to a `const` in every branch.
fn discriminator_key(branches: &[Value]) -> Option<String> {
    if branches.len() < 2 {
        return None;
    }

    let first_properties = branches.first()?.get("properties")?.as_object()?;

    first_properties
        .iter()
        .filter(|(_, schema)| schema.get("const").is_some())
        .map(|(name, _)| name.clone())
        .find(|name| {
            branches.iter().all(|branch| {
                branch
                    .get("properties")
                    .and_then(|properties| properties.get(name))
                    .and_then(|schema| schema.get("const"))
                    .is_some()
            })
        })
}

// [impl->swdd~ank-schema-provides-manifest-validation~1]
// Converts a JSON Pointer (`/a/b`) into the manifest's dot notation (`a.b`).
fn format_instance_path(pointer: &str) -> String {
    pointer
        .split('/')
        .skip_while(|segment| segment.is_empty())
        .map(|segment| segment.replace("~1", "/").replace("~0", "~"))
        .collect::<Vec<_>>()
        .join(".")
}

// [impl->swdd~ank-schema-provides-manifest-validation~1]
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

    // [impl->swdd~ank-schema-scopes-validation-errors~1]
    // `iter_errors` is used over `evaluate`, which drops `required` violations
    // when the same schema object also declares `properties`.
    let errors: Vec<String> = validator
        .iter_errors(instance)
        .map(|e| {
            format!(
                "'{}': {}",
                format_instance_path(&e.instance_path().to_string()),
                e
            )
        })
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

    // Generates the schema without the simplification pass, i.e. the schema as
    // it is exported to the JSON schema file consumed by editors.
    fn generate_raw_schema() -> serde_json::Value {
        let generator = SchemaSettings::draft07().into_generator();
        let schema = generator.into_root_schema_for::<StateSpec>();
        serde_json::to_value(&schema).unwrap()
    }

    fn build_validator(schema: &serde_json::Value) -> jsonschema::Validator {
        jsonschema::options()
            .with_draft(jsonschema::Draft::Draft7)
            .build(schema)
            .unwrap()
    }

    // Manifests covering valid and invalid variations of the constructs the
    // simplification pass rewrites (tagged rule enum, optional fields, ...).
    fn sample_manifests() -> Vec<serde_json::Value> {
        vec![
            // Valid, minimal.
            json!({
                "apiVersion": "v1",
                "workloads": {
                    "nginx": { "agent": "agent_A", "runtime": "podman", "runtimeConfig": "image: nginx" }
                }
            }),
            // Valid, both rule variants present.
            json!({
                "apiVersion": "v1",
                "workloads": {
                    "nginx": {
                        "agent": "agent_A", "runtime": "podman", "runtimeConfig": "image: nginx",
                        "controlInterfaceAccess": { "allowRules": [
                            { "type": "StateRule", "operation": "ReadWrite", "filterMasks": ["*"] },
                            { "type": "LogRule", "workloadNames": ["nginx"] }
                        ] }
                    }
                }
            }),
            // Invalid: StateRule with a mistyped field.
            json!({
                "apiVersion": "v1",
                "workloads": {
                    "nginx": {
                        "agent": "agent_A", "runtime": "podman", "runtimeConfig": "image: nginx",
                        "controlInterfaceAccess": { "allowRules": [
                            { "type": "StateRule", "operation": "ReadWrite", "filterMask": ["*"] }
                        ] }
                    }
                }
            }),
            // Invalid: unknown rule type.
            json!({
                "apiVersion": "v1",
                "workloads": {
                    "nginx": {
                        "agent": "agent_A", "runtime": "podman", "runtimeConfig": "image: nginx",
                        "controlInterfaceAccess": { "allowRules": [
                            { "type": "NotARule", "operation": "ReadWrite", "filterMasks": ["*"] }
                        ] }
                    }
                }
            }),
            // Invalid: LogRule missing required workloadNames.
            json!({
                "apiVersion": "v1",
                "workloads": {
                    "nginx": {
                        "agent": "agent_A", "runtime": "podman", "runtimeConfig": "image: nginx",
                        "controlInterfaceAccess": { "allowRules": [ { "type": "LogRule" } ] }
                    }
                }
            }),
            // Optional field explicitly null (exercises the nullable rewrite).
            json!({ "apiVersion": "v1", "workloads": null }),
            // Invalid workload name.
            json!({
                "apiVersion": "v1",
                "workloads": { "invalid.name": { "agent": "agent_A", "runtime": "podman", "runtimeConfig": "x" } }
            }),
            // Invalid: missing required runtime.
            json!({
                "apiVersion": "v1",
                "workloads": { "nginx": { "agent": "agent_A", "runtimeConfig": "image: nginx" } }
            }),
        ]
    }

    // [utest->swdd~ank-schema-scopes-validation-errors~1]
    #[test]
    fn utest_simplified_schema_accepts_same_manifests_as_raw_schema() {
        let raw_validator = build_validator(&generate_raw_schema());
        let simplified_validator = build_validator(&get_schema_value().unwrap());

        for manifest in sample_manifests() {
            assert_eq!(
                raw_validator.is_valid(&manifest),
                simplified_validator.is_valid(&manifest),
                "raw and simplified schema disagree on validity of: {manifest}"
            );
        }
    }

    // [utest->swdd~ank-schema-provides-manifest-validation~1]
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

    // [utest->swdd~ank-schema-provides-manifest-validation~1]
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

    // [utest->swdd~ank-schema-provides-manifest-validation~1]
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

    // [utest->swdd~ank-schema-provides-manifest-validation~1]
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

    // [utest->swdd~ank-schema-provides-manifest-validation~1]
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

    // [utest->swdd~ank-schema-scopes-validation-errors~1]
    #[test]
    fn utest_validate_manifest_state_rule_typo_reports_scoped_error() {
        let manifest = json!({
            "apiVersion": "v1",
            "workloads": {
                "nginx": {
                    "agent": "agent_A",
                    "runtime": "podman",
                    "runtimeConfig": "image: nginx:latest",
                    "controlInterfaceAccess": {
                        "allowRules": [
                            {
                                "type": "StateRule",
                                "operation": "ReadWrite",
                                // typo: must be "filterMasks"
                                "filterMask": ["*"]
                            }
                        ]
                    }
                }
            }
        });

        let error = validate_manifest(&manifest).unwrap_err();

        // The error must point at the offending rule and name the missing
        // property instead of leaking unrelated LogRule branch errors.
        assert!(
            error.contains("controlInterfaceAccess.allowRules.0"),
            "unexpected error: {error}"
        );
        assert!(
            error.contains("\"filterMasks\" is a required property"),
            "unexpected error: {error}"
        );
        assert!(
            !error.contains("LogRule"),
            "error should not mention the unrelated LogRule variant: {error}"
        );
        assert!(
            !error.contains("workloadNames"),
            "error should not mention the unrelated LogRule variant: {error}"
        );
        assert!(
            !error.contains("is not of type \"null\""),
            "error should not leak the nullable wrapper: {error}"
        );
    }

    // [utest->swdd~ank-schema-scopes-validation-errors~1]
    #[test]
    fn utest_validate_manifest_invalid_rule_type_reports_allowed_values() {
        let manifest = json!({
            "apiVersion": "v1",
            "workloads": {
                "nginx": {
                    "agent": "agent_A",
                    "runtime": "podman",
                    "runtimeConfig": "image: nginx:latest",
                    "controlInterfaceAccess": {
                        "allowRules": [
                            {
                                "type": "NotARule",
                                "operation": "ReadWrite",
                                "filterMasks": ["*"]
                            }
                        ]
                    }
                }
            }
        });

        let error = validate_manifest(&manifest).unwrap_err();
        assert!(
            error.contains("controlInterfaceAccess.allowRules.0.type"),
            "unexpected error: {error}"
        );
    }

    // [utest->swdd~ank-schema-scopes-validation-errors~1]
    #[test]
    fn utest_validate_manifest_valid_state_and_log_rules() {
        let manifest = json!({
            "apiVersion": "v1",
            "workloads": {
                "nginx": {
                    "agent": "agent_A",
                    "runtime": "podman",
                    "runtimeConfig": "image: nginx:latest",
                    "controlInterfaceAccess": {
                        "allowRules": [
                            {
                                "type": "StateRule",
                                "operation": "ReadWrite",
                                "filterMasks": ["*"]
                            },
                            {
                                "type": "LogRule",
                                "workloadNames": ["nginx"]
                            }
                        ]
                    }
                }
            }
        });

        assert!(validate_manifest(&manifest).is_ok());
    }

    // [utest->swdd~ank-schema-provides-manifest-validation~1]
    #[test]
    fn utest_format_instance_path_uses_dot_notation() {
        assert_eq!(format_instance_path(""), "");
        assert_eq!(
            format_instance_path("/workloads/nginx/agent"),
            "workloads.nginx.agent"
        );
        assert_eq!(
            format_instance_path("/workloads/nginx/controlInterfaceAccess/allowRules/0"),
            "workloads.nginx.controlInterfaceAccess.allowRules.0"
        );
        // JSON Pointer escapes are decoded per segment.
        assert_eq!(format_instance_path("/a~1b/c~0d"), "a/b.c~d");
    }
}
