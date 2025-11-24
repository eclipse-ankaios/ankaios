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

use super::workload::{
    verify_workload_name_length, verify_workload_name_not_empty, verify_workload_name_pattern,
};
use crate::ank_base::{
    AccessRightsRuleEnumSpec, AccessRightsRuleSpec, ControlInterfaceAccessSpec, LogRuleSpec,
    ReadWriteEnum, StateRuleSpec,
};

pub const WILDCARD_SYMBOL: &str = "*";

impl ControlInterfaceAccessSpec {
    // [impl->swdd~common-access-rules-filter-mask-convention~1]
    pub fn verify_format(&self) -> Result<(), String> {
        self.allow_rules
            .iter()
            .chain(self.deny_rules.iter())
            .try_for_each(|rule| rule.access_rights_rule_enum.verify_format())
    }
}

impl AccessRightsRuleSpec {
    pub fn state_rule(operation: ReadWriteEnum, filter_masks: Vec<String>) -> Self {
        AccessRightsRuleSpec {
            access_rights_rule_enum: AccessRightsRuleEnumSpec::StateRule(StateRuleSpec {
                operation,
                filter_masks,
            }),
        }
    }

    pub fn log_rule(workload_names: Vec<String>) -> Self {
        AccessRightsRuleSpec {
            access_rights_rule_enum: AccessRightsRuleEnumSpec::LogRule(LogRuleSpec {
                workload_names,
            }),
        }
    }

    pub fn verify_format(&self) -> Result<(), String> {
        self.access_rights_rule_enum.verify_format()
    }
}

impl AccessRightsRuleEnumSpec {
    fn verify_format(&self) -> Result<(), String> {
        match self {
            // [impl->swdd~common-access-rules-filter-mask-convention~1]
            AccessRightsRuleEnumSpec::StateRule(state_rule) => {
                state_rule.filter_masks.iter().try_for_each(|filter| {
                    if filter.is_empty() {
                        return Err(
                            "Empty filter masks are not allowed in Control Interface access rules"
                                .to_string(),
                        );
                    }
                    Ok(())
                })?;
            }
            // [impl->swdd~common-access-rules-logs-workload-names-convention~1]
            AccessRightsRuleEnumSpec::LogRule(log_rule) => {
                log_rule.workload_names.iter().try_for_each(|name| {
                    Self::verify_log_rule_workload_name_pattern_format(name)
                })?;
            }
        }
        Ok(())
    }

    // [impl->swdd~common-access-rules-logs-workload-names-convention~1]
    fn verify_log_rule_workload_name_pattern_format(workload_name: &str) -> Result<(), String> {
        if let Some(wildcard_pos) = workload_name.find(WILDCARD_SYMBOL) {
            let prefix = &workload_name[..wildcard_pos];
            let suffix = &workload_name[wildcard_pos + 1..];
            if suffix.contains(WILDCARD_SYMBOL) {
                Err(format!("Expected at most one '{WILDCARD_SYMBOL}' symbol."))
            } else {
                verify_workload_name_pattern(prefix)
                    .and_then(|_| verify_workload_name_pattern(suffix))
                    .and_then(|_| verify_workload_name_length(prefix.len() + suffix.len()))
            }
        } else {
            let length = workload_name.len();
            verify_workload_name_pattern(workload_name)
                .and_then(|_| verify_workload_name_length(length))
                .and_then(|_| verify_workload_name_not_empty(length))
        }
        .map_err(|err| format!("Unsupported workload name for log rule '{workload_name}'. {err}"))
    }
}

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_control_interface_access() -> ControlInterfaceAccessSpec {
    ControlInterfaceAccessSpec {
        allow_rules: vec![AccessRightsRuleSpec::state_rule(
            ReadWriteEnum::RwReadWrite,
            vec!["desiredState".to_string()],
        )],
        deny_rules: vec![AccessRightsRuleSpec::state_rule(
            ReadWriteEnum::RwWrite,
            vec!["desiredState.workload.workload_B".to_string()],
        )],
    }
}

#[cfg(test)]
mod tests {
    use crate::ank_base::{AccessRightsRuleSpec, ReadWriteEnum};
    use crate::test_utils::generate_test_control_interface_access;

    // [utest->swdd~common-access-rules-filter-mask-convention~1]
    #[test]
    fn utest_access_rights_state_rule_verify_fails() {
        let empty_state_rule =
            AccessRightsRuleSpec::state_rule(ReadWriteEnum::RwWrite, vec!["".to_string()]);

        assert!(empty_state_rule.verify_format().is_err_and(
            |x| x == "Empty filter masks are not allowed in Control Interface access rules"
        ));
    }

    // [utest->swdd~common-access-rules-filter-mask-convention~1]
    #[test]
    fn utest_access_rights_state_rule_verify_success() {
        let state_rule =
            AccessRightsRuleSpec::state_rule(ReadWriteEnum::RwWrite, vec!["some".to_string()]);

        assert!(state_rule.verify_format().is_ok());
    }

    // [utest->swdd~common-access-rules-logs-workload-names-convention~1]
    #[test]
    fn utest_access_rights_log_rule_verify_success() {
        const MAX_PREFIX: &str = "123456789012345678901234567890";
        const MAX_SUFFIX: &str = "123456789012345678901234567890123";

        assert!(log_rule_with_workload("workload_1").verify_format().is_ok());
        assert!(
            log_rule_with_workload("*workload_1")
                .verify_format()
                .is_ok()
        );
        assert!(
            log_rule_with_workload("work*load_1")
                .verify_format()
                .is_ok()
        );
        assert!(
            log_rule_with_workload("workload_1*")
                .verify_format()
                .is_ok()
        );
        assert!(
            log_rule_with_workload(&format!("{MAX_PREFIX}{MAX_SUFFIX}"))
                .verify_format()
                .is_ok()
        );
        assert!(
            log_rule_with_workload(&format!("*{MAX_PREFIX}{MAX_SUFFIX}"))
                .verify_format()
                .is_ok()
        );
        assert!(
            log_rule_with_workload(&format!("{MAX_PREFIX}*{MAX_SUFFIX}"))
                .verify_format()
                .is_ok()
        );
        assert!(
            log_rule_with_workload(&format!("{MAX_PREFIX}{MAX_SUFFIX}*"))
                .verify_format()
                .is_ok()
        );
    }

    // [utest->swdd~common-access-rules-logs-workload-names-convention~1]
    #[test]
    fn utest_access_rights_log_rule_verify_fails() {
        const TOO_LONG_PREFIX: &str = "123456789012345678901234567890";
        const TOO_LONG_SUFFIX: &str = "1234567890123456789012345678901234";

        assert!(log_rule_with_workload("").verify_format().is_err());
        assert!(
            log_rule_with_workload(&format!("{TOO_LONG_PREFIX}{TOO_LONG_SUFFIX}"))
                .verify_format()
                .is_err()
        );
        assert!(
            log_rule_with_workload(&format!("*{TOO_LONG_PREFIX}{TOO_LONG_SUFFIX}"))
                .verify_format()
                .is_err()
        );
        assert!(
            log_rule_with_workload(&format!("{TOO_LONG_PREFIX}*{TOO_LONG_SUFFIX}"))
                .verify_format()
                .is_err()
        );
        assert!(
            log_rule_with_workload(&format!("{TOO_LONG_PREFIX}{TOO_LONG_SUFFIX}*"))
                .verify_format()
                .is_err()
        );
        assert!(
            log_rule_with_workload("just.wrong")
                .verify_format()
                .is_err()
        );
        assert!(
            log_rule_with_workload("also@wrong")
                .verify_format()
                .is_err()
        );
        assert!(
            log_rule_with_workload("*also@wrong")
                .verify_format()
                .is_err()
        );
        assert!(
            log_rule_with_workload("al*so@wrong")
                .verify_format()
                .is_err()
        );
        assert!(
            log_rule_with_workload("also@wr*ong")
                .verify_format()
                .is_err()
        );
        assert!(
            log_rule_with_workload("also@wrong*")
                .verify_format()
                .is_err()
        );
        assert!(
            log_rule_with_workload("multiple*wildcards*wrong")
                .verify_format()
                .is_err()
        );
    }

    fn log_rule_with_workload(workload_name: &str) -> AccessRightsRuleSpec {
        AccessRightsRuleSpec::log_rule(vec![workload_name.to_string()])
    }

    // [utest->swdd~common-access-rules-filter-mask-convention~1]
    #[test]
    fn utest_control_interface_access_verify_fails_on_empty_allow_rule_filter() {
        let mut control_interface_access = generate_test_control_interface_access();

        let empty_state_rule =
            AccessRightsRuleSpec::state_rule(ReadWriteEnum::RwWrite, vec!["".to_string()]);

        control_interface_access
            .allow_rules
            .push(empty_state_rule.clone());
        assert!(control_interface_access.verify_format().is_err_and(
            |x| x == "Empty filter masks are not allowed in Control Interface access rules"
        ));
    }

    // [utest->swdd~common-access-rules-filter-mask-convention~1]
    #[test]
    fn utest_control_interface_access_verify_fails_on_empty_deny_rule_filter() {
        let mut control_interface_access = generate_test_control_interface_access();

        let empty_state_rule =
            AccessRightsRuleSpec::state_rule(ReadWriteEnum::RwWrite, vec!["".to_string()]);

        control_interface_access
            .deny_rules
            .push(empty_state_rule.clone());
        assert!(control_interface_access.verify_format().is_err_and(
            |x| x == "Empty filter masks are not allowed in Control Interface access rules"
        ));
    }

    // [utest->swdd~common-access-rules-filter-mask-convention~1]
    #[test]
    fn utest_control_interface_access_verify_success() {
        let control_interface_access = generate_test_control_interface_access();

        assert!(control_interface_access.verify_format().is_ok());
    }
}
