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

use ankaios_api::ank_base::{LogEntriesResponse, LogEntry};
use prost::Message;

// [impl->swdd~agent-checks-size-of-logs~1]

const LOGS_MAX_SIZE_MB: usize = 2;
const LOGS_MAX_SIZE_BYTES: usize = LOGS_MAX_SIZE_MB * 1024 * 1024;
const TRUNCATION_NOTICE: &str = " [truncated]";

pub fn process_log_entries_response(logs_response: LogEntriesResponse) -> Vec<LogEntriesResponse> {
    if logs_response.encoded_len() <= LOGS_MAX_SIZE_BYTES {
        return vec![logs_response];
    }

    let mut result = Vec::new();
    let mut current_response = LogEntriesResponse {
        log_entries: Vec::new(),
    };

    for entry in logs_response.log_entries {
        let entry_size = entry.encoded_len();

        if entry_size > LOGS_MAX_SIZE_BYTES {
            // Single entry exceeds limit - truncate message and add info entry
            if !current_response.log_entries.is_empty() {
                result.push(current_response);
                current_response = LogEntriesResponse {
                    log_entries: Vec::new(),
                };
            }
            let (truncated_entry, info_entry) = truncate_log_entry(entry);
            result.push(LogEntriesResponse {
                log_entries: vec![truncated_entry, info_entry],
            });
        } else if current_response.encoded_len() + entry_size > LOGS_MAX_SIZE_BYTES {
            // Adding this entry would exceed limit - start new response
            if !current_response.log_entries.is_empty() {
                result.push(current_response);
            }
            current_response = LogEntriesResponse {
                log_entries: vec![entry],
            };
        } else {
            // Entry fits in current response
            current_response.log_entries.push(entry);
        }
    }

    if !current_response.log_entries.is_empty() {
        result.push(current_response);
    }

    result
}

fn truncate_log_entry(entry: LogEntry) -> (LogEntry, LogEntry) {
    // Create the info entry first to know its size
    let info_entry = LogEntry {
        workload_name: entry.workload_name.clone(),
        message: format!(
            "The previous message was truncated due to exceeding the maximum {LOGS_MAX_SIZE_MB} MB size."
        ),
    };
    let info_entry_size = info_entry.encoded_len();

    // Calculate overhead (everything except the message content)
    let message_len = entry.message.len();
    let overhead = entry.encoded_len() - message_len;

    // Protobuf overhead for repeated field entries (tag + length prefix per entry)
    const REPEATED_FIELD_OVERHEAD: usize = 10;

    // Calculate max message size to fit within limit (accounting for info entry and protobuf overhead)
    let max_message_size = LOGS_MAX_SIZE_BYTES
        .saturating_sub(overhead)
        .saturating_sub(TRUNCATION_NOTICE.len())
        .saturating_sub(info_entry_size)
        .saturating_sub(REPEATED_FIELD_OVERHEAD);

    // Truncate at char boundary
    let truncated_message = if max_message_size < message_len {
        let mut end = max_message_size;
        while end > 0 && !entry.message.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}{}", &entry.message[..end], TRUNCATION_NOTICE)
    } else {
        entry.message
    };

    let truncated_entry = LogEntry {
        workload_name: entry.workload_name,
        message: truncated_message,
    };

    (truncated_entry, info_entry)
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
    use ankaios_api::ank_base::WorkloadInstanceName;
    use ankaios_api::test_utils::fixtures;

    // [utest->swdd~agent-checks-size-of-logs~1]

    fn generate_test_log_entry(message: &str) -> LogEntry {
        LogEntry {
            workload_name: Some(WorkloadInstanceName {
                agent_name: fixtures::AGENT_NAMES[0].into(),
                workload_name: fixtures::WORKLOAD_NAMES[0].into(),
                id: fixtures::WORKLOAD_IDS[0].into(),
            }),
            message: message.into(),
        }
    }

    fn generate_test_log_entry_with_size(target_size: usize) -> LogEntry {
        let entry = generate_test_log_entry("");
        let overhead = entry.encoded_len();
        let message = "A".repeat(target_size.saturating_sub(overhead));
        generate_test_log_entry(&message)
    }

    #[test]
    fn utest_process_multiple_entries_under_limit() {
        let entry_size = LOGS_MAX_SIZE_BYTES / 4;
        let entry1 = generate_test_log_entry_with_size(entry_size);
        let entry2 = generate_test_log_entry_with_size(entry_size);
        let entry3 = generate_test_log_entry_with_size(entry_size);

        let response = LogEntriesResponse {
            log_entries: vec![entry1.clone(), entry2.clone(), entry3.clone()],
        };

        let result = process_log_entries_response(response);

        assert_eq!(result.len(), 1, "The response should be the same");
        assert_eq!(result[0].log_entries.len(), 3);
        assert_eq!(result[0].log_entries[0], entry1);
        assert_eq!(result[0].log_entries[1], entry2);
        assert_eq!(result[0].log_entries[2], entry3);
        assert!(result[0].encoded_len() <= LOGS_MAX_SIZE_BYTES);
    }

    #[test]
    fn utest_process_entries_together_exceed_limit() {
        let entry_size = LOGS_MAX_SIZE_BYTES / 2 + 1000;
        let entry1 = generate_test_log_entry_with_size(entry_size);
        let entry2 = generate_test_log_entry_with_size(entry_size);
        assert!(entry1.encoded_len() <= LOGS_MAX_SIZE_BYTES);
        assert!(entry2.encoded_len() <= LOGS_MAX_SIZE_BYTES);

        let response = LogEntriesResponse {
            log_entries: vec![entry1.clone(), entry2.clone()],
        };
        assert!(response.encoded_len() > LOGS_MAX_SIZE_BYTES);

        let result = process_log_entries_response(response);

        assert_eq!(result.len(), 2, "Should be split in 2 responses.");
        assert_eq!(result[0].log_entries.len(), 1);
        assert_eq!(result[1].log_entries.len(), 1);

        assert_eq!(
            result[0].log_entries[0], entry1,
            "Entry should be unchanged"
        );
        assert_eq!(
            result[1].log_entries[0], entry2,
            "Entry should be unchanged"
        );

        assert!(result[0].encoded_len() <= LOGS_MAX_SIZE_BYTES);
        assert!(result[1].encoded_len() <= LOGS_MAX_SIZE_BYTES);
    }

    #[test]
    fn utest_process_single_entry_exceeds_limit() {
        let large_message = "A".repeat(LOGS_MAX_SIZE_BYTES + 1000);
        let entry = generate_test_log_entry(&large_message);
        let original_workload_name = entry.workload_name.clone();
        assert!(entry.encoded_len() > LOGS_MAX_SIZE_BYTES);

        let response = LogEntriesResponse {
            log_entries: vec![entry],
        };

        let result = process_log_entries_response(response);

        assert_eq!(result.len(), 1, "Should still be only one response");
        assert_eq!(
            result[0].log_entries.len(),
            2,
            "There should be an additional entry"
        );

        assert!(
            result[0].log_entries[0]
                .message
                .ends_with(TRUNCATION_NOTICE)
        );
        assert_eq!(
            result[0].log_entries[0].workload_name,
            original_workload_name
        );

        assert!(
            result[0].log_entries[1]
                .message
                .contains(&format!("{LOGS_MAX_SIZE_MB} MB"))
        );
        assert_eq!(
            result[0].log_entries[1].workload_name,
            original_workload_name
        );

        assert!(result[0].encoded_len() <= LOGS_MAX_SIZE_BYTES);
    }

    #[test]
    fn utest_process_two_entries_both_exceed_limit() {
        let large_message1 = "B".repeat(LOGS_MAX_SIZE_BYTES + 500);
        let large_message2 = "C".repeat(LOGS_MAX_SIZE_BYTES + 800);
        let entry1 = generate_test_log_entry(&large_message1);
        let entry2 = generate_test_log_entry(&large_message2);

        // Verify precondition: both entries exceed limit
        assert!(entry1.encoded_len() > LOGS_MAX_SIZE_BYTES);
        assert!(entry2.encoded_len() > LOGS_MAX_SIZE_BYTES);

        let response = LogEntriesResponse {
            log_entries: vec![entry1, entry2],
        };

        let result = process_log_entries_response(response);

        assert_eq!(result.len(), 2, "There should be two responses");
        assert_eq!(
            result[0].log_entries.len(),
            2,
            "An additional entry should be present"
        );
        assert_eq!(
            result[1].log_entries.len(),
            2,
            "An additional entry should be present"
        );

        assert!(
            result[0].log_entries[0]
                .message
                .ends_with(TRUNCATION_NOTICE)
        );
        assert!(result[0].log_entries[1].message.contains("truncated"));

        assert!(
            result[1].log_entries[0]
                .message
                .ends_with(TRUNCATION_NOTICE)
        );
        assert!(result[1].log_entries[1].message.contains("truncated"));

        assert!(result[0].encoded_len() <= LOGS_MAX_SIZE_BYTES);
        assert!(result[1].encoded_len() <= LOGS_MAX_SIZE_BYTES);
    }

    #[test]
    fn utest_process_small_entry_and_large_entry() {
        let small_entry = generate_test_log_entry("small message");
        let large_message = "D".repeat(LOGS_MAX_SIZE_BYTES + 1000);
        let large_entry = generate_test_log_entry(&large_message);

        assert!(small_entry.encoded_len() <= LOGS_MAX_SIZE_BYTES);
        assert!(large_entry.encoded_len() > LOGS_MAX_SIZE_BYTES);

        let response = LogEntriesResponse {
            log_entries: vec![small_entry.clone(), large_entry],
        };

        let result = process_log_entries_response(response);

        assert_eq!(result.len(), 2, "Should be split into 2 responses");

        assert_eq!(result[0].log_entries.len(), 1);
        assert_eq!(result[0].log_entries[0], small_entry);

        assert_eq!(result[1].log_entries.len(), 2);
        assert!(
            result[1].log_entries[0]
                .message
                .ends_with(TRUNCATION_NOTICE)
        );
        assert!(result[1].log_entries[1].message.contains("truncated"));

        assert!(result[0].encoded_len() <= LOGS_MAX_SIZE_BYTES);
        assert!(result[1].encoded_len() <= LOGS_MAX_SIZE_BYTES);
    }
}
