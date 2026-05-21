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

#[cfg_attr(test, mockall_double::double)]
use crate::runtime_connectors::cli_command::CliCommand;

#[cfg(test)]
use mockall::automock;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemdUnitState {
    pub active_state: String,
    pub sub_state: String,
    pub exit_code: Option<i32>,
}

pub struct SystemdCli {}

#[cfg_attr(test, automock)]
#[allow(dead_code)]
impl SystemdCli {
    const SYSTEMCTL_CMD: &str = "systemctl";
    /// Start a systemd unit
    pub async fn start_unit(unit_name: &str) -> Result<(), String> {
        log::debug!("Starting systemd unit: {}", unit_name);
        CliCommand::new(Self::SYSTEMCTL_CMD)
            .args(&["start", unit_name])
            .exec()
            .await?;
        Ok(())
    }

    /// Stop a systemd unit
    pub async fn stop_unit(unit_name: &str) -> Result<(), String> {
        log::debug!("Stopping systemd unit: {}", unit_name);
        CliCommand::new(Self::SYSTEMCTL_CMD)
            .args(&["stop", unit_name])
            .exec()
            .await?;
        Ok(())
    }

    /// Restart a systemd unit
    pub async fn restart_unit(unit_name: &str) -> Result<(), String> {
        log::debug!("Restarting systemd unit: {}", unit_name);
        CliCommand::new(Self::SYSTEMCTL_CMD)
            .args(&["restart", unit_name])
            .exec()
            .await?;
        Ok(())
    }

    /// Get the state of a systemd unit
    pub async fn get_unit_state(unit_name: &str) -> Result<SystemdUnitState, String> {
        log::trace!("Getting state for systemd unit: {}", unit_name);

        let output = CliCommand::new(Self::SYSTEMCTL_CMD)
            .args(&[
                "show",
                unit_name,
                "--property=ActiveState,SubState,ExecMainStatus",
            ])
            .exec()
            .await?;

        Self::parse_unit_state(&output)
    }

    /// Get the uptime of a systemd unit in seconds
    /// Returns None if the service is not active or timestamp cannot be parsed
    pub async fn get_unit_uptime_seconds(unit_name: &str) -> Result<Option<u64>, String> {
        log::trace!("Getting uptime for systemd unit: {}", unit_name);

        let systemctl_output = CliCommand::new(Self::SYSTEMCTL_CMD)
            .args(&[
                "show",
                unit_name,
                "--property=ActiveEnterTimestampMonotonic,ActiveState",
            ])
            .exec()
            .await?;

        // Parse systemctl output
        let mut active_enter_monotonic: Option<u64> = None;
        let mut is_active = false;

        for line in systemctl_output.lines() {
            let line = line.trim();
            if let Some(value) = line.strip_prefix("ActiveEnterTimestampMonotonic=") {
                // Monotonic timestamp is in microseconds
                active_enter_monotonic = value.parse().ok();
            } else if let Some(value) = line.strip_prefix("ActiveState=") {
                is_active = value == "active";
            }
        }

        if !is_active || active_enter_monotonic == Some(0) {
            return Ok(None);
        }

        if let Some(enter_us) = active_enter_monotonic {
            let uptime_output = tokio::fs::read_to_string("/proc/uptime")
                .await
                .map_err(|e| format!("Failed to read /proc/uptime: {}", e))?;

            // /proc/uptime gives system uptime in seconds (first field)
            let uptime_str = uptime_output.split_whitespace().next()
                .ok_or_else(|| "Failed to parse /proc/uptime".to_string())?;
            let system_uptime_secs: f64 = uptime_str.parse()
                .map_err(|_| "Invalid uptime format".to_string())?;
            let system_uptime_us = (system_uptime_secs * 1_000_000.0) as u64;

            // Calculate service uptime
            let service_uptime_us = system_uptime_us.saturating_sub(enter_us);
            let service_uptime_secs = service_uptime_us / 1_000_000;

            Ok(Some(service_uptime_secs))
        } else {
            Ok(None)
        }
    }

    /// Parse systemctl show output into SystemdUnitState
    fn parse_unit_state(output: &str) -> Result<SystemdUnitState, String> {
        let mut active_state = None;
        let mut sub_state = None;
        let mut exit_code = None;

        for line in output.lines() {
            let line = line.trim();
            if let Some(value) = line.strip_prefix("ActiveState=") {
                active_state = Some(value.to_string());
            } else if let Some(value) = line.strip_prefix("SubState=") {
                sub_state = Some(value.to_string());
            } else if let Some(value) = line.strip_prefix("ExecMainStatus=") {
                exit_code = value.parse().ok();
            }
        }

        Ok(SystemdUnitState {
            active_state: active_state.ok_or_else(|| "Missing ActiveState".to_string())?,
            sub_state: sub_state.ok_or_else(|| "Missing SubState".to_string())?,
            exit_code,
        })
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
    fn test_parse_unit_state_running() {
        let output = "ActiveState=active\nSubState=running\nExecMainStatus=0\n";
        let state = SystemdCli::parse_unit_state(output).unwrap();

        assert_eq!(state.active_state, "active");
        assert_eq!(state.sub_state, "running");
        assert_eq!(state.exit_code, Some(0));
    }

    #[test]
    fn test_parse_unit_state_failed() {
        let output = "ActiveState=failed\nSubState=failed\nExecMainStatus=1\n";
        let state = SystemdCli::parse_unit_state(output).unwrap();

        assert_eq!(state.active_state, "failed");
        assert_eq!(state.sub_state, "failed");
        assert_eq!(state.exit_code, Some(1));
    }

    #[test]
    fn test_parse_unit_state_inactive() {
        let output = "ActiveState=inactive\nSubState=dead\nExecMainStatus=0\n";
        let state = SystemdCli::parse_unit_state(output).unwrap();

        assert_eq!(state.active_state, "inactive");
        assert_eq!(state.sub_state, "dead");
        assert_eq!(state.exit_code, Some(0));
    }

    #[test]
    fn test_parse_unit_state_missing_field() {
        let output = "ActiveState=active\n";
        let result = SystemdCli::parse_unit_state(output);

        assert!(result.is_err());
    }

}
