// Copyright (c) 2026 Red Hat LLC
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

//! Filesystem persistence of individual workloads as standalone manifest files under
//! `<persistence_dir>/workloads/<workload_name>.yaml`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use ankaios_sdk::{Manifest, Workload};
use common::path_security::safe_join;
use serde_yaml::Value;
use tokio::io::AsyncReadExt;

/// Maximum size for a persisted workload file (3 MB).
/// Keeps restored manifests within the default control-interface/gRPC message limits.
const MAX_WORKLOAD_FILE_SIZE: u64 = 3 * 1024 * 1024;

/// Stores and restores individual workloads as standalone manifest files.
pub struct PersistenceStore {
    workloads_dir: PathBuf,
}

impl PersistenceStore {
    pub fn new(persistence_dir: &Path) -> Self {
        Self {
            workloads_dir: persistence_dir.join("workloads"),
        }
    }

    pub async fn init(&self) -> std::io::Result<()> {
        tokio::fs::create_dir_all(&self.workloads_dir).await
    }

    pub async fn persisted_names(&self) -> HashSet<String> {
        let mut names = HashSet::new();
        let Ok(mut entries) = tokio::fs::read_dir(&self.workloads_dir).await else {
            return names;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            if let Some(file_name) = entry.file_name().to_str() {
                if !file_name.starts_with('.') {
                    if let Some(workload_name) = file_name.strip_suffix(".yaml") {
                        names.insert(workload_name.to_owned());
                    }
                }
            }
        }
        names
    }

    pub async fn persist(
        &self,
        name: &str,
        workload: &Workload,
        configs: &HashMap<String, Value>,
    ) -> Result<(), String> {
        tokio::fs::create_dir_all(&self.workloads_dir)
            .await
            .map_err(|e| format!("Failed to create workloads directory: {e}"))?;

        let content = serde_yaml::to_string(&manifest_dict(name, workload, configs))
            .map_err(|e| format!("Failed to serialize YAML: {e}"))?
            .into_bytes();

        let workload_file = safe_join(&self.workloads_dir, &format!("{name}.yaml"))
            .map_err(|e| format!("Invalid workload name '{name}': {e}"))?;
        let workloads_dir = self.workloads_dir.clone();

        tokio::task::spawn_blocking(move || write_atomically(&workloads_dir, &workload_file, &content))
            .await
            .map_err(|e| format!("Persist task panicked: {e}"))??;

        log::info!("Persisted workload '{name}'");
        Ok(())
    }

    pub async fn remove(&self, name: &str) -> Result<(), String> {
        let yaml_file = safe_join(&self.workloads_dir, &format!("{name}.yaml"))
            .map_err(|e| format!("Invalid workload name '{name}': {e}"))?;
        if yaml_file.exists() {
            tokio::fs::remove_file(&yaml_file)
                .await
                .map_err(|e| format!("Failed to remove {yaml_file:?}: {e}"))?;
            log::info!("Removed persisted workload '{name}'");
        }
        Ok(())
    }

    /// Loads all persisted workloads into a single [`Manifest`] ready to be applied,
    /// or `None` if nothing is persisted (or nothing could be loaded).
    pub async fn load_manifest(&self) -> Option<Manifest> {
        if !self.workloads_dir.exists() {
            return None;
        }

        let mut workloads = serde_yaml::Mapping::new();
        let mut configs = serde_yaml::Mapping::new();

        let mut entries = tokio::fs::read_dir(&self.workloads_dir).await.ok()?;
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("yaml") {
                // Temp files written by write_atomically (e.g. ".tmpAbC123") have no
                // extension and are already excluded by this check.
                continue;
            }

            match read_workload_file(&path).await {
                Ok(doc) => merge_workload_file(&mut workloads, &mut configs, doc),
                Err(e) => log::warn!("Skipping {path:?}: {e}"),
            }
        }

        if workloads.is_empty() {
            return None;
        }

        let mut doc = serde_yaml::Mapping::new();
        doc.insert(Value::String("apiVersion".to_owned()), Value::String("v1".to_owned()));
        doc.insert(Value::String("workloads".to_owned()), Value::Mapping(workloads));
        doc.insert(Value::String("configs".to_owned()), Value::Mapping(configs));

        let content = serde_yaml::to_string(&doc).ok()?;
        match Manifest::from_string(content) {
            Ok(manifest) => Some(manifest),
            Err(e) => {
                log::error!("Failed to build manifest from persisted workloads: {e}");
                None
            }
        }
    }
}

fn manifest_dict(name: &str, workload: &Workload, configs: &HashMap<String, Value>) -> serde_yaml::Mapping {
    let mut workloads = serde_yaml::Mapping::new();
    workloads.insert(Value::String(name.to_owned()), Value::Mapping(workload.to_dict()));

    let mut configs_dict = serde_yaml::Mapping::new();
    for (config_name, value) in configs {
        configs_dict.insert(Value::String(config_name.clone()), value.clone());
    }

    let mut doc = serde_yaml::Mapping::new();
    doc.insert(Value::String("apiVersion".to_owned()), Value::String("v1".to_owned()));
    doc.insert(Value::String("workloads".to_owned()), Value::Mapping(workloads));
    doc.insert(Value::String("configs".to_owned()), Value::Mapping(configs_dict));
    doc
}

fn merge_workload_file(
    workloads: &mut serde_yaml::Mapping,
    configs: &mut serde_yaml::Mapping,
    doc: serde_yaml::Mapping,
) {
    if let Some(Value::Mapping(wls)) = doc.get(Value::String("workloads".to_owned())) {
        for (key, value) in wls {
            workloads.insert(key.clone(), value.clone());
        }
    }
    if let Some(Value::Mapping(cfgs)) = doc.get(Value::String("configs".to_owned())) {
        for (key, value) in cfgs {
            configs.insert(key.clone(), value.clone());
        }
    }
}

async fn read_workload_file(path: &Path) -> Result<serde_yaml::Mapping, String> {
    let file = tokio::fs::File::open(path)
        .await
        .map_err(|e| format!("Failed to open: {e}"))?;
    let metadata = file
        .metadata()
        .await
        .map_err(|e| format!("Failed to stat: {e}"))?;
    if metadata.len() > MAX_WORKLOAD_FILE_SIZE {
        return Err(format!(
            "File too large ({} bytes, max {MAX_WORKLOAD_FILE_SIZE})",
            metadata.len()
        ));
    }

    let mut content = String::new();
    file.take(MAX_WORKLOAD_FILE_SIZE)
        .read_to_string(&mut content)
        .await
        .map_err(|e| format!("Failed to read: {e}"))?;

    match serde_yaml::from_str::<Value>(&content) {
        Ok(Value::Mapping(map)) => Ok(map),
        Ok(_) => Err("File does not contain a YAML mapping".to_owned()),
        Err(e) => Err(format!("Failed to parse YAML: {e}")),
    }
}

/// Writes `content` to `workloads_dir` under a temp name and atomically renames it to
/// `final_path`, syncing both the file and the directory entry. Runs on a blocking thread
/// pool since it performs synchronous filesystem I/O.
fn write_atomically(workloads_dir: &Path, final_path: &Path, content: &[u8]) -> Result<(), String> {
    use std::fs::File;
    use std::io::Write;

    // NamedTempFile creates a secure temp file (O_EXCL, random name) preventing
    // symlink attacks. It auto-cleans on drop if persist() is not called.
    let mut temp = tempfile::NamedTempFile::new_in(workloads_dir)
        .map_err(|e| format!("Failed to create temp file: {e}"))?;

    temp.write_all(content)
        .map_err(|e| format!("Failed to write temp file: {e}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temp.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("Failed to set permissions: {e}"))?;
    }

    temp.as_file()
        .sync_all()
        .map_err(|e| format!("Failed to sync temp file: {e}"))?;

    temp.persist(final_path)
        .map_err(|e| format!("Failed to persist workload file: {e}"))?;

    // Sync the directory entry so the rename itself is durable.
    File::open(workloads_dir)
        .and_then(|dir| dir.sync_all())
        .map_err(|e| format!("Failed to sync workloads directory: {e}"))?;

    Ok(())
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
    use ankaios_sdk::Workload as SdkWorkload;

    fn test_workload(name: &str) -> Workload {
        SdkWorkload::builder()
            .workload_name(name)
            .agent_name("agent_A")
            .runtime("podman")
            .runtime_config("image: nginx")
            .build()
            .expect("valid workload")
    }

    #[tokio::test]
    async fn utest_persist_creates_file() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let store = PersistenceStore::new(temp_dir.path());
        let workload = test_workload("nginx");

        store.persist("nginx", &workload, &HashMap::new()).await.unwrap();

        assert!(temp_dir.path().join("workloads/nginx.yaml").exists());
    }

    #[tokio::test]
    async fn utest_remove_deletes_file() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let store = PersistenceStore::new(temp_dir.path());
        let workload = test_workload("nginx");
        store.persist("nginx", &workload, &HashMap::new()).await.unwrap();

        store.remove("nginx").await.unwrap();

        assert!(!temp_dir.path().join("workloads/nginx.yaml").exists());
    }

    #[tokio::test]
    async fn utest_persisted_names_reflects_disk_state() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let store = PersistenceStore::new(temp_dir.path());
        store.persist("nginx", &test_workload("nginx"), &HashMap::new()).await.unwrap();
        store.persist("redis", &test_workload("redis"), &HashMap::new()).await.unwrap();

        let names = store.persisted_names().await;

        assert_eq!(names.len(), 2);
        assert!(names.contains("nginx"));
        assert!(names.contains("redis"));
    }

    #[tokio::test]
    async fn utest_load_manifest_merges_all_persisted_workloads() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let store = PersistenceStore::new(temp_dir.path());
        store.persist("nginx", &test_workload("nginx"), &HashMap::new()).await.unwrap();
        store.persist("redis", &test_workload("redis"), &HashMap::new()).await.unwrap();

        let manifest = store.load_manifest().await.expect("manifest expected");

        let state = ankaios_sdk::CompleteState::new_from_manifest(manifest);
        assert!(state.get_workload("nginx").is_some());
        assert!(state.get_workload("redis").is_some());
    }

    #[tokio::test]
    async fn utest_load_manifest_none_when_empty() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let store = PersistenceStore::new(temp_dir.path());
        store.init().await.unwrap();

        assert!(store.load_manifest().await.is_none());
    }

    #[tokio::test]
    async fn utest_load_manifest_skips_corrupted_file() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let store = PersistenceStore::new(temp_dir.path());
        store.init().await.unwrap();
        tokio::fs::write(
            temp_dir.path().join("workloads/corrupted.yaml"),
            "{ invalid yaml [[[",
        )
        .await
        .unwrap();

        assert!(store.load_manifest().await.is_none());
    }

    #[tokio::test]
    async fn utest_persist_includes_referenced_configs() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let store = PersistenceStore::new(temp_dir.path());
        let workload = SdkWorkload::builder()
            .workload_name("nginx")
            .agent_name("agent_A")
            .runtime("podman")
            .runtime_config("image: nginx")
            .add_config("port", "web_server_port")
            .build()
            .expect("valid workload");
        let configs = HashMap::from([(
            "web_server_port".to_owned(),
            Value::String("8081".to_owned()),
        )]);

        store.persist("nginx", &workload, &configs).await.unwrap();

        let content = tokio::fs::read_to_string(temp_dir.path().join("workloads/nginx.yaml"))
            .await
            .unwrap();
        assert!(content.contains("web_server_port"));
        assert!(content.contains("8081"));
    }
}
