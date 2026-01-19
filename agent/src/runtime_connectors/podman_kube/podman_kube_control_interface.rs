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

use super::podman_kube_runtime_config::PodmanKubeRuntimeConfig;
use crate::runtime_connectors::{RuntimeError, podman_cli::API_PIPES_MOUNT_POINT};

use ankaios_api::ank_base::WorkloadNamed;

use serde::Deserialize;
use serde_yaml::{Deserializer, Mapping, Value};
use std::path::Path;

const TARGET_PATH_LENGTH: usize = 2;
const POD_PATH_INDEX: usize = 0;
const CONTAINER_PATH_INDEX: usize = 1;

pub(super) struct ControlInterfaceTarget {
    pub pod: String,
    pub container: String,
}

impl ControlInterfaceTarget {
    pub(super) fn from_podman_kube_runtime_config(
        config: &PodmanKubeRuntimeConfig,
    ) -> Result<Option<ControlInterfaceTarget>, RuntimeError> {
        let Some(target_path) = &config.control_interface_target else {
            return Ok(None);
        };

        log::trace!("Parsing control interface target path: '{target_path}'");

        let composite_parts: Vec<String> = target_path.split('/').map(|s| s.to_owned()).collect();
        if composite_parts.len() != TARGET_PATH_LENGTH {
            return Err(RuntimeError::Unsupported(format!(
                "Invalid control interface target format: '{target_path}'. Expected format: '<pod_name>/<container_name>'"
            )));
        }

        Ok(Some(ControlInterfaceTarget {
            pod: composite_parts[POD_PATH_INDEX].clone(),
            container: composite_parts[CONTAINER_PATH_INDEX].clone(),
        }))
    }
}

// [impl->swdd~podman-kube-mounts-control-interface~1]
pub(super) fn add_control_interface(
    workload_config: &mut PodmanKubeRuntimeConfig,
    workload_named: &WorkloadNamed,
    control_interface_target: &ControlInterfaceTarget,
    control_interface_path: &Path,
) -> Result<(), RuntimeError> {
    log::trace!(
        "Adding control interface for workload '{}'",
        workload_named.instance_name
    );

    let mut manifests = parse_yaml_manifests(&workload_config.manifest)?;

    add_control_interface_in_correct_manifest(
        &mut manifests,
        workload_named,
        control_interface_target,
        control_interface_path,
    )?;

    workload_config.manifest = manifests
        .into_iter()
        .map(|manifest| serialize_yaml_manifest(&manifest))
        .collect::<Result<Vec<String>, RuntimeError>>()?
        .join("---\n");
    Ok(())
}

fn parse_yaml_manifests(manifest_str: &str) -> Result<Vec<Value>, RuntimeError> {
    let mut manifests = Vec::new();

    for manifest_result in Deserializer::from_str(manifest_str) {
        let manifest = Value::deserialize(manifest_result).map_err(|e| {
            RuntimeError::Unsupported(format!("Failed to parse YAML manifest: {e}"))
        })?;

        log::trace!("Parsed manifest: {manifest:#?}");

        manifests.push(manifest);
    }

    Ok(manifests)
}

fn add_control_interface_in_correct_manifest(
    manifests: &mut Vec<Value>,
    workload_named: &WorkloadNamed,
    control_interface_target: &ControlInterfaceTarget,
    control_interface_path: &Path,
) -> Result<(), RuntimeError> {
    log::trace!(
        "Processing {} manifests for workload '{}'",
        manifests.len(),
        workload_named.instance_name
    );

    for manifest in manifests {
        if should_inject_control_interface(manifest, &control_interface_target.pod)? {
            inject_control_interface(
                manifest,
                workload_named,
                &control_interface_target.container,
                control_interface_path,
            )?;
            return Ok(());
        }
    }

    log::warn!(
        "No matching manifest found to inject control interface for workload '{}'",
        workload_named.instance_name
    );
    Ok(())
}

fn get_metadata_name(manifest: &Value) -> Result<&str, RuntimeError> {
    manifest
        .get("metadata")
        .and_then(|m| m.get("name"))
        .and_then(|n| n.as_str())
        .ok_or_else(|| {
            log::warn!("Manifest missing metadata.name field");
            RuntimeError::Unsupported("Manifest missing metadata.name".to_string())
        })
}

// [impl->swdd~podman-kube-validates-target-path-format~1]
fn should_inject_control_interface(
    manifest: &Value,
    target_pod_name: &String,
) -> Result<bool, RuntimeError> {
    log::trace!("Checking if manifest matches target pod name '{target_pod_name}'");

    let kind = manifest
        .get("kind")
        .and_then(|k| k.as_str())
        .ok_or_else(|| {
            log::warn!("Manifest missing 'kind' field");
            RuntimeError::Unsupported("Manifest missing 'kind' field".to_string())
        })?;

    match kind {
        "Pod" => {
            let pod_name = get_metadata_name(manifest)?;

            Ok(pod_name == target_pod_name)
        }
        "Deployment" => {
            // For Podman, a Deployment with metadata.name '<name>' results in a pod named '<name>-pod'.
            let deployment_name = get_metadata_name(manifest)?;

            Ok(format!("{deployment_name}-pod") == *target_pod_name)
        }
        _ => {
            log::trace!("Skipping manifest with kind '{kind}'");
            Ok(false)
        }
    }
}

// [impl->swdd~podman-kube-mounts-control-interface~1]
// [impl->swdd~podman-kube-injects-control-interface-volume-mount~1]
fn inject_control_interface(
    manifest: &mut Value,
    workload_named: &WorkloadNamed,
    container_name: &str,
    control_interface_path: &Path,
) -> Result<(), RuntimeError> {
    log::debug!(
        "Injecting control interface into manifest for workload '{}'",
        workload_named.instance_name
    );
    inject_volume_mount(manifest, container_name)?;
    inject_control_volume(manifest, control_interface_path)?;

    log::trace!("Manifest after injecting control interface: {manifest:#?}");
    Ok(())
}

fn find_containers_spec(value: &mut Value) -> Option<&mut Value> {
    let spec = value.get_mut("spec")?;

    // In the Pod case the containers object is directly here
    if spec.get("containers").is_some() {
        return Some(spec);
    }

    // In the Deployment case the containers object is under template.spec
    spec.get_mut("template")?.get_mut("spec")
}

// [impl->swdd~podman-kube-injects-control-interface-volume-mount~1]
fn inject_volume_mount(
    manifest: &mut Value,
    target_container_name: &str,
) -> Result<(), RuntimeError> {
    let containers = find_containers_spec(manifest)
        .and_then(|spec| spec.get_mut("containers"))
        .and_then(|c| c.as_sequence_mut())
        .ok_or_else(|| RuntimeError::Unsupported("Manifest missing spec.containers".to_string()))?;

    for container in containers {
        let container_name = container
            .get("name")
            .and_then(|n| n.as_str())
            .ok_or_else(|| RuntimeError::Unsupported("Container missing name field".to_string()))?;

        if container_name == target_container_name {
            add_control_interface_mount(container);
            break;
        }
    }

    Ok(())
}

// [impl->swdd~podman-kube-injects-control-interface-volume-mount~1]
fn add_control_interface_mount(container: &mut Value) {
    let container_mapping = container.as_mapping_mut().unwrap();
    let vol_mounts_key = Value::from("volumeMounts");

    if !container_mapping.contains_key(&vol_mounts_key) {
        container_mapping.insert(vol_mounts_key.clone(), Value::Sequence(Vec::new()));
    }

    let vol_mounts = container_mapping
        .get_mut(&vol_mounts_key)
        .and_then(|v| v.as_sequence_mut())
        .unwrap();

    let mut volume_mount = Mapping::new();
    volume_mount.insert(Value::from("name"), Value::from("control-interface-volume"));
    volume_mount.insert(Value::from("mountPath"), Value::from(API_PIPES_MOUNT_POINT));

    vol_mounts.push(Value::Mapping(volume_mount));
}

// [impl->swdd~podman-kube-mounts-control-interface~1]
// [impl->swdd~podman-kube-injects-control-interface-volume~1]
fn inject_control_volume(
    manifest: &mut Value,
    control_interface_path: &Path,
) -> Result<(), RuntimeError> {
    let spec_mapping = find_containers_spec(manifest)
        .and_then(|s| s.as_mapping_mut())
        .ok_or_else(|| RuntimeError::Unsupported("Manifest missing spec".to_string()))?;

    let volumes_key = Value::from("volumes");
    if !spec_mapping.contains_key(&volumes_key) {
        spec_mapping.insert(volumes_key.clone(), Value::Sequence(Vec::new()));
    }

    let volumes = spec_mapping
        .get_mut(&volumes_key)
        .and_then(|v| v.as_sequence_mut())
        .ok_or_else(|| RuntimeError::Unsupported("Manifest missing spec.volumes".to_string()))?;

    let volume = create_control_interface_volume(control_interface_path);
    volumes.push(volume);
    Ok(())
}

// [impl->swdd~podman-kube-injects-control-interface-volume~1]
// [impl->swdd~podman-kube-mounts-control-interface~1]
fn create_control_interface_volume(control_interface_path: &Path) -> Value {
    let mut host_path = Mapping::new();
    host_path.insert(
        Value::from("path"),
        Value::from(control_interface_path.to_string_lossy().to_string()),
    );
    host_path.insert(Value::from("type"), Value::from("Directory"));

    let mut volume = Mapping::new();
    volume.insert(Value::from("name"), Value::from("control-interface-volume"));
    volume.insert(Value::from("hostPath"), Value::Mapping(host_path));

    Value::Mapping(volume)
}

fn serialize_yaml_manifest(manifest: &Value) -> Result<String, RuntimeError> {
    serde_yaml::to_string(manifest)
        .map_err(|e| RuntimeError::Unsupported(format!("Failed to serialize manifest: {e}")))
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

    use crate::runtime_connectors::podman_kube::podman_kube_runtime::PODMAN_KUBE_RUNTIME_NAME;

    use crate::runtime_connectors::RuntimeError;
    use crate::runtime_connectors::podman_kube::podman_kube_runtime_config::PodmanKubeRuntimeConfig;

    use ankaios_api::ank_base::{AccessRightsRuleSpec, ReadWriteEnum, WorkloadNamed, WorkloadSpec};
    use ankaios_api::test_utils::{
        fixtures, generate_test_workload_named_with_runtime_config,
        generate_test_workload_with_runtime_config,
    };

    use serde_yaml::Value;

    fn generate_test_podman_kube_workload() -> WorkloadNamed {
        generate_test_workload_named_with_runtime_config(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            PODMAN_KUBE_RUNTIME_NAME,
            r#"{"generalOptions": ["-gen", "--eral"], "playOptions": ["-pl", "--ay"], "downOptions": ["-do", "--wn"], "manifest": "kube_config"}"#,
        )
    }

    #[test]
    fn utest_control_interface_target_valid() {
        let manifest_str = r#"
apiVersion: v1
kind: Pod
metadata:
  name: test-pod
spec:
  containers:
  - name: test-container
    image: test-image
"#;
        let runtime_config = format!(
            r#"{{"generalOptions": ["-gen", "--eral"], "playOptions": ["-pl", "--ay"], "downOptions": ["-do", "--wn"], controlInterfaceTarget: "test-pod/test-container", "manifest": {manifest_str:?}}}"#
        );
        let mut workload: WorkloadSpec = generate_test_workload_with_runtime_config(
            fixtures::AGENT_NAMES[0].to_string(),
            PODMAN_KUBE_RUNTIME_NAME.to_string(),
            runtime_config,
        );
        workload.files = Default::default();

        let workload_config = PodmanKubeRuntimeConfig::try_from(&workload).unwrap();

        let target = ControlInterfaceTarget::from_podman_kube_runtime_config(&workload_config);
        assert!(
            matches!(target, Ok(Some(target)) if target.pod == "test-pod" && target.container == "test-container")
        );
    }

    #[test]
    fn utest_control_interface_target_invalid() {
        let manifest_str = r#"
apiVersion: v1
kind: Pod
metadata:
  name: test-pod
spec:
  containers:
  - name: test-container
    image: test-image
"#;
        let runtime_config = format!(
            r#"{{"generalOptions": ["-gen", "--eral"], "playOptions": ["-pl", "--ay"], "downOptions": ["-do", "--wn"], controlInterfaceTarget: "test-pod-test-container", "manifest": {manifest_str:?}}}"#
        );
        let mut workload: WorkloadSpec = generate_test_workload_with_runtime_config(
            fixtures::AGENT_NAMES[0].to_string(),
            PODMAN_KUBE_RUNTIME_NAME.to_string(),
            runtime_config,
        );
        workload.files = Default::default();

        let workload_config = PodmanKubeRuntimeConfig::try_from(&workload).unwrap();

        let target = ControlInterfaceTarget::from_podman_kube_runtime_config(&workload_config);
        assert!(target.is_err());
    }

    #[test]
    fn utest_control_interface_target_missing() {
        let manifest_str = r#"
apiVersion: v1
kind: Pod
metadata:
  name: test-pod
spec:
  containers:
  - name: test-container
    image: test-image
"#;
        let runtime_config = format!(
            r#"{{"generalOptions": ["-gen", "--eral"], "playOptions": ["-pl", "--ay"], "downOptions": ["-do", "--wn"], "manifest": {manifest_str:?}}}"#
        );
        let mut workload: WorkloadSpec = generate_test_workload_with_runtime_config(
            fixtures::AGENT_NAMES[0].to_string(),
            PODMAN_KUBE_RUNTIME_NAME.to_string(),
            runtime_config,
        );
        workload.files = Default::default();

        let workload_config = PodmanKubeRuntimeConfig::try_from(&workload).unwrap();
        let target = ControlInterfaceTarget::from_podman_kube_runtime_config(&workload_config);
        assert!(matches!(target, Ok(None)));
    }

    // [utest->swdd~podman-kube-mounts-control-interface~1]
    #[test]
    fn utest_target_path_ignored_when_no_access_rules() {
        let manifest_str = r#"
apiVersion: v1
kind: Pod
metadata:
  name: test-pod
spec:
  containers:
  - name: test-container
    image: test-image
    volumeMounts: []
  volumes: []
"#;
        let runtime_config = format!(
            r#"{{"generalOptions": ["-gen", "--eral"], "playOptions": ["-pl", "--ay"], "downOptions": ["-do", "--wn"], controlInterfaceTarget: "test-pod/test-container", "manifest": {manifest_str:?}}}"#
        );

        let mut workload: WorkloadSpec = generate_test_workload_with_runtime_config(
            fixtures::AGENT_NAMES[0].to_string(),
            PODMAN_KUBE_RUNTIME_NAME.to_string(),
            runtime_config,
        );
        workload.files = Default::default();
        workload.control_interface_access = Default::default();

        assert!(!workload.needs_control_interface());

        let workload_config = PodmanKubeRuntimeConfig::try_from(&workload).unwrap();

        assert_eq!(workload_config.manifest, manifest_str);
        assert!(
            !workload_config
                .manifest
                .contains("control-interface-volume")
        );
    }

    // [utest->swdd~podman-kube-validates-target-path-format~1]
    #[test]
    fn utest_parse_yaml_manifests_simple_manifest() {
        let manifest_str = r#"
apiVersion: v1
kind: Pod
metadata:
  name: test-pod
"#;

        let result = parse_yaml_manifests(manifest_str);
        assert!(result.is_ok());
        let manifests = result.unwrap();
        assert_eq!(manifests.len(), 1);
        assert_eq!(manifests[0]["kind"], "Pod");
        assert_eq!(manifests[0]["metadata"]["name"], "test-pod");
    }

    #[test]
    fn utest_parse_yaml_manifests_multiple_manifests() {
        let manifest_str = r#"
apiVersion: v1
kind: Pod
metadata:
  name: test-pod1
---
apiVersion: v1
kind: Service
metadata:
  name: test-service
"#;

        let result = parse_yaml_manifests(manifest_str);
        assert!(result.is_ok());
        let manifests = result.unwrap();
        assert_eq!(manifests.len(), 2);
        assert_eq!(manifests[0]["kind"], "Pod");
        assert_eq!(manifests[1]["kind"], "Service");
    }

    #[test]
    fn utest_parse_yaml_manifests_invalid_yaml() {
        let manifest_str = "invalid: yaml: content: [";

        let result = parse_yaml_manifests(manifest_str);
        assert!(result.is_err());
        assert!(matches!(result, Err(RuntimeError::Unsupported(_))));
    }

    #[test]
    fn utest_get_metadata_name_success() {
        let manifest = serde_yaml::from_str::<Value>(
            r#"
apiVersion: v1
kind: Pod
metadata:
  name: test-pod
"#,
        )
        .unwrap();

        let result = get_metadata_name(&manifest);
        assert!(matches!(result, Ok("test-pod")));
    }

    #[test]
    fn utest_get_metadata_name_missing_metadata() {
        let manifest = serde_yaml::from_str::<Value>(
            r#"
apiVersion: v1
kind: Pod
"#,
        )
        .unwrap();

        let result = get_metadata_name(&manifest);
        assert!(result.is_err());
        assert!(matches!(result, Err(RuntimeError::Unsupported(_))));
    }

    #[test]
    fn utest_get_metadata_name_missing_name() {
        let manifest = serde_yaml::from_str::<Value>(
            r#"
apiVersion: v1
kind: Pod
metadata:
  namespace: default
"#,
        )
        .unwrap();

        let result = get_metadata_name(&manifest);
        assert!(result.is_err());
        assert!(matches!(result, Err(RuntimeError::Unsupported(_))));
    }

    #[test]
    fn utest_get_metadata_name_name_not_string() {
        let manifest = serde_yaml::from_str::<Value>(
            r#"
apiVersion: v1
kind: Pod
metadata:
  name:
    nested: true
"#,
        )
        .unwrap();

        let result = get_metadata_name(&manifest);
        assert!(result.is_err());
        assert!(matches!(result, Err(RuntimeError::Unsupported(_))));
    }

    // [utest->swdd~podman-kube-injects-control-interface-volume~1]
    #[test]
    fn utest_should_inject_control_interface_not_pod_kind() {
        let manifest = serde_yaml::from_str::<Value>(
            r#"
apiVersion: v1
kind: Service
metadata:
  name: test-service
"#,
        )
        .unwrap();

        let result = should_inject_control_interface(&manifest, &"test-service".to_owned());
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    // [utest->swdd~podman-kube-mounts-control-interface~1]
    #[test]
    fn utest_should_inject_control_interface_wrong_pod_name() {
        let manifest = serde_yaml::from_str::<Value>(
            r#"
apiVersion: v1
kind: Pod
metadata:
  name: different-pod
"#,
        )
        .unwrap();

        let result = should_inject_control_interface(&manifest, &"test-pod".to_owned());
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    // [utest->swdd~podman-kube-mounts-control-interface~1]
    #[test]
    fn utest_should_inject_control_interface_matching_pod() {
        let manifest = serde_yaml::from_str::<Value>(
            r#"
apiVersion: v1
kind: Pod
metadata:
  name: test-pod
"#,
        )
        .unwrap();

        let result = should_inject_control_interface(&manifest, &"test-pod".to_owned());
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn utest_should_inject_control_interface_matching_deployment() {
        let manifest = serde_yaml::from_str::<Value>(
            r#"
apiVersion: v1
kind: Deployment
metadata:
  name: pod_A
"#,
        )
        .unwrap();

        let result = should_inject_control_interface(&manifest, &"pod_A-pod".to_owned());
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn utest_should_inject_control_interface_wrong_deployment_name() {
        let manifest = serde_yaml::from_str::<Value>(
            r#"
apiVersion: v1
kind: Deployment
metadata:
  name: different
"#,
        )
        .unwrap();

        let result = should_inject_control_interface(&manifest, &"pod_A-pod".to_owned());
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    fn utest_should_inject_control_interface_missing_kind() {
        let manifest = serde_yaml::from_str::<Value>(
            r#"
apiVersion: v1
metadata:
  name: test-pod
"#,
        )
        .unwrap();

        let result = should_inject_control_interface(&manifest, &"test-pod".to_owned());
        assert!(result.is_err());
        assert!(matches!(result, Err(RuntimeError::Unsupported(_))));
    }

    #[test]
    fn utest_should_inject_control_interface_missing_metadata_name() {
        let manifest = serde_yaml::from_str::<Value>(
            r#"
apiVersion: v1
kind: Pod
metadata:
  namespace: default
"#,
        )
        .unwrap();

        let result = should_inject_control_interface(&manifest, &"test-pod".to_owned());
        assert!(result.is_err());
        assert!(matches!(result, Err(RuntimeError::Unsupported(_))));
    }

    #[test]
    fn utest_should_inject_control_interface_deployment_missing_metadata_name() {
        let manifest = serde_yaml::from_str::<Value>(
            r#"
apiVersion: v1
kind: Deployment
metadata:
  namespace: default
"#,
        )
        .unwrap();

        let result = should_inject_control_interface(&manifest, &"pod_A-pod".to_owned());
        assert!(result.is_err());
        assert!(matches!(result, Err(RuntimeError::Unsupported(_))));
    }

    // [utest->swdd~podman-kube-injects-control-interface-volume-mount~1]
    #[test]
    fn utest_inject_control_interface_success() {
        let manifest_str = r#"
apiVersion: v1
kind: Pod
metadata:
  name: test-pod
spec:
  containers:
  - name: test-container
    image: test-image
    volumeMounts: []
  volumes: []
"#;
        let runtime_config = format!(
            r#"{{"generalOptions": ["-gen", "--eral"], "playOptions": ["-pl", "--ay"], "downOptions": ["-do", "--wn"], controlInterfaceTarget: "test-pod/test-container", "manifest": {manifest_str:?}}}"#
        );

        let mut workload = generate_test_podman_kube_workload();
        workload.workload.runtime_config = runtime_config;
        workload.workload.files = Default::default();
        workload.workload.control_interface_access.allow_rules =
            vec![AccessRightsRuleSpec::state_rule(
                ReadWriteEnum::RwReadWrite,
                vec!["desiredState".to_string()],
            )];

        let mut workload_config = PodmanKubeRuntimeConfig::try_from(&workload.workload).unwrap();
        let control_interface_target =
            ControlInterfaceTarget::from_podman_kube_runtime_config(&workload_config)
                .unwrap()
                .unwrap();

        let run_folder = std::path::PathBuf::from("/run-folder");
        let control_interface_path = workload
            .instance_name
            .pipes_folder_name(run_folder.as_path())
            .join("control_interface");

        assert!(
            add_control_interface(
                &mut workload_config,
                &workload,
                &control_interface_target,
                control_interface_path.as_path(),
            )
            .is_ok()
        );

        assert!(
            workload_config
                .manifest
                .contains("control-interface-volume")
        );

        let parsed: Value = serde_yaml::from_str(&workload_config.manifest).unwrap();
        let vol_mounts = &parsed["spec"]["containers"][0]["volumeMounts"];
        assert!(!vol_mounts.as_sequence().unwrap().is_empty());

        let volumes = &parsed["spec"]["volumes"];
        assert!(!volumes.as_sequence().unwrap().is_empty());
    }

    // [utest->swdd~podman-kube-injects-control-interface-volume-mount~1]
    #[test]
    fn utest_inject_volume_mount_success() {
        let mut manifest = serde_yaml::from_str::<Value>(
            r#"
apiVersion: v1
kind: Pod
spec:
  containers:
  - name: test-container
    image: test-image
    volumeMounts: []
  - name: other-container
    image: other-image
    volumeMounts: []
"#,
        )
        .unwrap();

        let result = inject_volume_mount(&mut manifest, "test-container");
        assert!(result.is_ok());

        let vol_mounts = &manifest["spec"]["containers"][0]["volumeMounts"];
        let vol_mount_list = vol_mounts.as_sequence().unwrap();
        assert_eq!(vol_mount_list.len(), 1);
        assert_eq!(vol_mount_list[0]["name"], "control-interface-volume");
        assert_eq!(vol_mount_list[0]["mountPath"], API_PIPES_MOUNT_POINT);

        let other_vol_mounts = &manifest["spec"]["containers"][1]["volumeMounts"];
        assert_eq!(other_vol_mounts.as_sequence().unwrap().len(), 0);
    }

    // [utest->swdd~podman-kube-injects-control-interface-volume-mount~1]
    #[test]
    fn utest_inject_volume_mount_missing_containers() {
        let mut manifest = serde_yaml::from_str::<Value>(
            r#"
apiVersion: v1
kind: Pod
spec:
  volumes: []
"#,
        )
        .unwrap();

        let result = inject_volume_mount(&mut manifest, "test-container");
        assert!(result.is_err());
        assert!(matches!(result, Err(RuntimeError::Unsupported(_))));
    }

    // [utest->swdd~podman-kube-injects-control-interface-volume-mount~1]
    #[test]
    fn utest_inject_volume_mount_container_missing_name() {
        let mut manifest = serde_yaml::from_str::<Value>(
            r#"
apiVersion: v1
kind: Pod
spec:
  containers:
  - image: test-image
    volumeMounts: []
"#,
        )
        .unwrap();

        let result = inject_volume_mount(&mut manifest, "test-container");
        assert!(result.is_err());
        assert!(matches!(result, Err(RuntimeError::Unsupported(_))));
    }

    // [utest->swdd~podman-kube-injects-control-interface-volume-mount~1]
    #[test]
    fn utest_add_control_interface_mount_with_existing_mounts() {
        let mut container = serde_yaml::from_str::<Value>(
            r#"
name: test-container
volumeMounts:
- name: existing-volume
  mountPath: /existing/path
"#,
        )
        .unwrap();

        add_control_interface_mount(&mut container);

        let vol_mounts = container["volumeMounts"].as_sequence().unwrap();
        assert_eq!(vol_mounts.len(), 2);
        assert_eq!(vol_mounts[1]["name"], "control-interface-volume");
        assert_eq!(vol_mounts[1]["mountPath"], API_PIPES_MOUNT_POINT);
    }

    // [utest->swdd~podman-kube-injects-control-interface-volume-mount~1]
    #[test]
    fn utest_add_control_interface_mount_no_volume_mounts() {
        let mut container = serde_yaml::from_str::<Value>(
            r#"
name: test-container
image: test-image
"#,
        )
        .unwrap();

        add_control_interface_mount(&mut container);

        let vol_mounts = container["volumeMounts"].as_sequence().unwrap();
        assert_eq!(vol_mounts.len(), 1);
        assert_eq!(vol_mounts[0]["name"], "control-interface-volume");
        assert_eq!(vol_mounts[0]["mountPath"], API_PIPES_MOUNT_POINT);
    }

    // [utest->swdd~podman-kube-injects-control-interface-volume~1]
    #[test]
    fn utest_inject_control_volume_success_with_existing_volume_wl() {
        let mut manifest = serde_yaml::from_str::<Value>(
            r#"
apiVersion: v1
kind: Pod
spec:
  containers: []
  volumes:
  - name: existing-volume
    emptyDir: {}
"#,
        )
        .unwrap();

        let control_interface_path = std::path::PathBuf::from("/some/control_interface");

        let result = inject_control_volume(&mut manifest, control_interface_path.as_path());
        assert!(result.is_ok());

        let volumes = manifest["spec"]["volumes"].as_sequence().unwrap();
        assert_eq!(volumes.len(), 2);
        assert_eq!(volumes[1]["name"], "control-interface-volume");
        assert!(volumes[1]["hostPath"].is_mapping());
    }

    // [utest->swdd~podman-kube-injects-control-interface-volume-mount~1]
    #[test]
    fn utest_inject_control_volume_success_with_missing_volume_wl() {
        let mut manifest = serde_yaml::from_str::<Value>(
            r#"
apiVersion: v1
kind: Pod
spec:
  containers: []
"#,
        )
        .unwrap();

        let control_interface_path = std::path::PathBuf::from("/some/control_interface");

        let result = inject_control_volume(&mut manifest, control_interface_path.as_path());
        assert!(result.is_ok());

        let volumes = manifest["spec"]["volumes"].as_sequence().unwrap();
        assert_eq!(volumes.len(), 1);
        assert_eq!(volumes[0]["name"], "control-interface-volume");
        assert!(volumes[0]["hostPath"].is_mapping());
    }

    // [utest->swdd~podman-kube-injects-control-interface-volume~1]
    #[test]
    fn utest_create_control_interface_volume() {
        let control_interface_path = std::path::PathBuf::from("/some/control_interface");

        let volume = create_control_interface_volume(control_interface_path.as_path());

        assert_eq!(volume["name"], "control-interface-volume");
        assert!(volume["hostPath"].is_mapping());
        assert_eq!(volume["hostPath"]["type"], "Directory");

        assert_eq!(
            volume["hostPath"]["path"],
            control_interface_path.to_string_lossy().to_string()
        );
    }

    #[test]
    fn utest_serialize_yaml_manifest_success() {
        let manifest = serde_yaml::from_str::<Value>(
            r#"
apiVersion: v1
kind: Pod
metadata:
  name: test-pod
"#,
        )
        .unwrap();

        let result = serialize_yaml_manifest(&manifest);
        assert!(result.is_ok());

        let serialized = result.unwrap();
        assert!(serialized.contains("apiVersion: v1"));
        assert!(serialized.contains("kind: Pod"));
        assert!(serialized.contains("name: test-pod"));
    }

    // [utest->swdd~podman-kube-injects-control-interface-volume~1]
    #[test]
    fn utest_process_manifest_list_mixed_manifests() {
        let pod_manifest = serde_yaml::from_str::<Value>(
            r#"
apiVersion: v1
kind: Pod
metadata:
  name: target-pod
spec:
  containers:
  - name: target-container
    image: test-image
    volumeMounts: []
  volumes: []
"#,
        )
        .unwrap();

        let service_manifest = serde_yaml::from_str::<Value>(
            r#"
apiVersion: v1
kind: Service
metadata:
  name: test-service
spec:
  ports:
  - port: 80
"#,
        )
        .unwrap();

        let mut manifests = vec![pod_manifest, service_manifest];
        let mut workload = generate_test_podman_kube_workload();

        workload.workload.control_interface_access.allow_rules =
            vec![AccessRightsRuleSpec::state_rule(
                ReadWriteEnum::RwReadWrite,
                vec!["desiredState".to_string()],
            )];

        let control_interface_target = ControlInterfaceTarget {
            pod: "target-pod".to_string(),
            container: "target-container".to_string(),
        };

        let control_interface_path = std::path::PathBuf::from("/some/control_interface");

        let result = add_control_interface_in_correct_manifest(
            &mut manifests,
            &workload,
            &control_interface_target,
            control_interface_path.as_path(),
        );

        assert!(result.is_ok());

        assert_eq!(manifests.len(), 2);

        let injected_mounts = manifests[0]
            .get("spec")
            .and_then(|s| s.get("containers"))
            .and_then(|c| c.get(0))
            .and_then(|c0| c0.get("volumeMounts"))
            .and_then(|v| v.as_sequence())
            .unwrap();
        assert!(injected_mounts.iter().any(|m| {
            m.get("name").and_then(|n| n.as_str()) == Some("control-interface-volume")
                && m.get("mountPath").and_then(|p| p.as_str()) == Some(API_PIPES_MOUNT_POINT)
        }));

        let injected_volumes = manifests[0]
            .get("spec")
            .and_then(|s| s.get("volumes"))
            .and_then(|v| v.as_sequence())
            .unwrap();
        assert!(injected_volumes.iter().any(|v| {
            v.get("name").and_then(|n| n.as_str()) == Some("control-interface-volume")
        }));

        let service_serialized = serialize_yaml_manifest(&manifests[1]).unwrap();
        assert!(service_serialized.contains("kind: Service"));
        assert!(!service_serialized.contains("control-interface-volume"));
    }

    // [utest->swdd~podman-kube-injects-control-interface-volume~1]
    #[test]
    fn utest_process_manifests_with_control_interface_multiple_documents() {
        let manifest_str = r#"
apiVersion: v1
kind: Pod
metadata:
  name: target-pod
spec:
  containers:
  - name: target-container
    image: test-image
    volumeMounts: []
  volumes: []
---
apiVersion: v1
kind: Service
metadata:
  name: test-service
spec:
  ports:
  - port: 80
"#;
        let runtime_config = format!(
            r#"{{"generalOptions": ["-gen", "--eral"], "playOptions": ["-pl", "--ay"], "downOptions": ["-do", "--wn"], controlInterfaceTarget: "target-pod/target-container", "manifest": {manifest_str:?}}}"#
        );

        let mut workload = generate_test_podman_kube_workload();

        workload.workload.runtime_config = runtime_config;
        workload.workload.files = Default::default();
        workload.workload.control_interface_access.allow_rules =
            vec![AccessRightsRuleSpec::state_rule(
                ReadWriteEnum::RwReadWrite,
                vec!["desiredState".to_string()],
            )];

        let mut workload_config = PodmanKubeRuntimeConfig::try_from(&workload.workload).unwrap();
        let control_interface_target =
            ControlInterfaceTarget::from_podman_kube_runtime_config(&workload_config)
                .unwrap()
                .unwrap();

        let run_folder = std::path::PathBuf::from("/run-folder");
        let control_interface_path = workload
            .instance_name
            .pipes_folder_name(run_folder.as_path())
            .join("control_interface");

        assert!(
            add_control_interface(
                &mut workload_config,
                &workload,
                &control_interface_target,
                control_interface_path.as_path(),
            )
            .is_ok()
        );

        assert!(workload_config.manifest.contains("---\n"));
        assert!(
            workload_config
                .manifest
                .contains("control-interface-volume")
        );
        assert!(workload_config.manifest.contains("kind: Service"));
    }
}
