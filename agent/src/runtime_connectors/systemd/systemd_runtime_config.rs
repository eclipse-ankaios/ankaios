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

use super::systemd_runtime::SYSTEMD_RUNTIME_NAME;
use ankaios_api::ank_base::WorkloadSpec;

#[derive(Debug, serde::Deserialize, Eq, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SystemdRuntimeConfig {
    pub service_name: String,
    #[serde(default = "default_desired_state")]
    pub desired_state: ServiceState,
}

fn default_desired_state() -> ServiceState {
    ServiceState::Running
}

#[derive(Debug, serde::Deserialize, Eq, PartialEq, Clone)]
#[serde(rename_all = "lowercase")]
pub enum ServiceState {
    Running,
    Stopped,
    Restarted,
}

const VALID_UNIT_SUFFIXES: &[&str] = &[
    ".service", ".timer", ".socket", ".mount", ".target", ".path", ".scope", ".slice", ".swap",
    ".automount", ".device",
];

fn validate_service_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("service_name must not be empty".to_string());
    }

    if !name.chars().all(|c| c.is_ascii_alphanumeric() || "@_:.-".contains(c)) {
        return Err(format!(
            "service_name '{}' contains invalid characters (only alphanumeric, @, _, :, ., - allowed)",
            name
        ));
    }

    if !VALID_UNIT_SUFFIXES.iter().any(|suffix| name.ends_with(suffix)) {
        return Err(format!(
            "service_name '{}' must end with a valid systemd unit suffix (e.g. .service, .timer)",
            name
        ));
    }

    Ok(())
}

impl TryFrom<&WorkloadSpec> for SystemdRuntimeConfig {
    type Error = String;
    fn try_from(workload_spec: &WorkloadSpec) -> Result<Self, Self::Error> {
        if SYSTEMD_RUNTIME_NAME != workload_spec.runtime {
            return Err(format!(
                "Received a workload for the wrong runtime: '{}'",
                workload_spec.runtime
            ));
        }
        let config: Self =
            serde_yaml::from_str(&workload_spec.runtime_config).map_err(|e| e.to_string())?;
        validate_service_name(&config.service_name)?;
        Ok(config)
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
    use ankaios_api::test_utils::generate_test_workload_with_params;

    const DIFFERENT_RUNTIME_NAME: &str = "different-runtime-name";

    #[test]
    fn utest_systemd_config_failure_missing_service_name() {
        let mut workload = generate_test_workload_with_params("agent_A", SYSTEMD_RUNTIME_NAME);
        workload.runtime_config = "something: without_service_name".to_string();

        assert!(SystemdRuntimeConfig::try_from(&workload).is_err());
    }

    #[test]
    fn utest_systemd_config_failure_wrong_runtime() {
        let workload = generate_test_workload_with_params("agent_A", DIFFERENT_RUNTIME_NAME);

        assert!(SystemdRuntimeConfig::try_from(&workload).is_err());
    }

    #[test]
    fn utest_systemd_config_success_minimal() {
        let mut workload = generate_test_workload_with_params("agent_A", SYSTEMD_RUNTIME_NAME);
        workload.runtime_config = "serviceName: nginx.service\n".to_string();

        let config = SystemdRuntimeConfig::try_from(&workload).unwrap();
        assert_eq!(config.service_name, "nginx.service");
        assert_eq!(config.desired_state, ServiceState::Running); // default
    }

    #[test]
    fn utest_systemd_config_success_with_desired_state_running() {
        let mut workload = generate_test_workload_with_params("agent_A", SYSTEMD_RUNTIME_NAME);
        workload.runtime_config =
            "serviceName: nginx.service\ndesiredState: running\n".to_string();

        let config = SystemdRuntimeConfig::try_from(&workload).unwrap();
        assert_eq!(config.service_name, "nginx.service");
        assert_eq!(config.desired_state, ServiceState::Running);
    }

    #[test]
    fn utest_systemd_config_success_with_desired_state_stopped() {
        let mut workload = generate_test_workload_with_params("agent_A", SYSTEMD_RUNTIME_NAME);
        workload.runtime_config =
            "serviceName: cups.service\ndesiredState: stopped\n".to_string();

        let config = SystemdRuntimeConfig::try_from(&workload).unwrap();
        assert_eq!(config.service_name, "cups.service");
        assert_eq!(config.desired_state, ServiceState::Stopped);
    }

    #[test]
    fn utest_systemd_config_success_with_desired_state_restarted() {
        let mut workload = generate_test_workload_with_params("agent_A", SYSTEMD_RUNTIME_NAME);
        workload.runtime_config =
            "serviceName: nginx.service\ndesiredState: restarted\n".to_string();

        let config = SystemdRuntimeConfig::try_from(&workload).unwrap();
        assert_eq!(config.service_name, "nginx.service");
        assert_eq!(config.desired_state, ServiceState::Restarted);
    }

    #[test]
    fn utest_systemd_config_success_with_template_unit() {
        let mut workload = generate_test_workload_with_params("agent_A", SYSTEMD_RUNTIME_NAME);
        workload.runtime_config = "serviceName: foo@bar.service\n".to_string();

        let config = SystemdRuntimeConfig::try_from(&workload).unwrap();
        assert_eq!(config.service_name, "foo@bar.service");
    }

    #[test]
    fn utest_systemd_config_success_with_timer_unit() {
        let mut workload = generate_test_workload_with_params("agent_A", SYSTEMD_RUNTIME_NAME);
        workload.runtime_config = "serviceName: backup.timer\n".to_string();

        let config = SystemdRuntimeConfig::try_from(&workload).unwrap();
        assert_eq!(config.service_name, "backup.timer");
    }

    #[test]
    fn utest_systemd_config_failure_empty_service_name() {
        let mut workload = generate_test_workload_with_params("agent_A", SYSTEMD_RUNTIME_NAME);
        workload.runtime_config = "serviceName: ''\n".to_string();

        let result = SystemdRuntimeConfig::try_from(&workload);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must not be empty"));
    }

    #[test]
    fn utest_systemd_config_failure_shell_metacharacters() {
        let mut workload = generate_test_workload_with_params("agent_A", SYSTEMD_RUNTIME_NAME);
        workload.runtime_config = "serviceName: \"test;rm -rf /.service\"\n".to_string();

        let result = SystemdRuntimeConfig::try_from(&workload);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid characters"));
    }

    #[test]
    fn utest_systemd_config_failure_no_unit_suffix() {
        let mut workload = generate_test_workload_with_params("agent_A", SYSTEMD_RUNTIME_NAME);
        workload.runtime_config = "serviceName: nginx\n".to_string();

        let result = SystemdRuntimeConfig::try_from(&workload);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("valid systemd unit suffix"));
    }

    #[test]
    fn utest_systemd_config_failure_path_traversal() {
        let mut workload = generate_test_workload_with_params("agent_A", SYSTEMD_RUNTIME_NAME);
        workload.runtime_config = "serviceName: \"../etc/passwd.service\"\n".to_string();

        let result = SystemdRuntimeConfig::try_from(&workload);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid characters"));
    }
}
