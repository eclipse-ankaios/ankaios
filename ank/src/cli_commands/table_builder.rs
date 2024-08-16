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

#[cfg(not(test))]
use crate::log::terminal_width;

#[cfg(test)]
fn terminal_width() -> usize {
    80
}

use tabled::{
    settings::{object::Columns, Modify, Padding, Style, Width},
    Table, Tabled,
};

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct AnkTableError(String);

impl fmt::Display for AnkTableError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Could not create table: {}", self.0)
    }
}

pub struct AnkTable<'a, RowType> {
    rows: &'a [RowType],
    table: Table,
}

impl<'a, RowType> AnkTable<'a, RowType>
where
    RowType: Tabled,
{
    const TRUNCATED_COLUMN_SUFFIX: &'static str = "...";
    const FIRST_COLUMN_POS: usize = 0;
    const ZERO_PADDING: usize = 0;

    pub fn new(rows: &'a [RowType]) -> Self {
        let table = Table::new(rows);
        Self { rows, table }
    }

    pub fn create_default_table(mut self) -> String {
        self.table = Table::new(self.rows);
        self.style_blank();
        self.disable_surrounding_padding();
        self.table.to_string()
    }

    pub fn table_with_wrapped_column_to_remaining_terminal_width(
        mut self,
        column_position: usize,
    ) -> Result<String, AnkTableError> {
        self.style_blank();
        self.disable_surrounding_padding();
        let total_table_width: usize = self.table.total_width();
        let available_column_width =
            self.terminal_width_for_column(column_position, total_table_width)?;

        self.table.with(
            Modify::new(Columns::single(column_position)).with(Width::wrap(available_column_width)),
        );
        Ok(self.table.to_string())
    }

    pub fn table_with_truncated_column_to_remaining_terminal_width(
        mut self,
        column_position: usize,
    ) -> Result<String, AnkTableError> {
        self.style_blank();
        self.disable_surrounding_padding();

        let total_table_width: usize = self.table.total_width();
        let available_column_width =
            self.terminal_width_for_column(column_position, total_table_width)?;
        self.table.with(
            Modify::new(Columns::single(column_position)).with(
                Width::truncate(available_column_width).suffix(Self::TRUNCATED_COLUMN_SUFFIX),
            ),
        );
        Ok(self.table.to_string())
    }

    fn style_blank(&mut self) {
        self.table.with(Style::blank());
    }

    fn disable_surrounding_padding(&mut self) {
        let column_count = self.table.count_columns();
        let last_column_pos = column_count - 1;

        let first_column_default_padding = self
            .table
            .get_config()
            .get_padding(tabled::grid::config::Entity::Column(Self::FIRST_COLUMN_POS));

        let last_column_default_padding = self
            .table
            .get_config()
            .get_padding(tabled::grid::config::Entity::Column(last_column_pos));

        /* Set the left padding of the first and the right padding of the last column to zero
        to align the table content to the full terminal width for better output quality. */
        self.table
            .with(Modify::new(Columns::first()).with(Padding::new(
                Self::ZERO_PADDING,
                first_column_default_padding.right.size,
                first_column_default_padding.top.size,
                first_column_default_padding.bottom.size,
            )))
            .with(Modify::new(Columns::last()).with(Padding::new(
                last_column_default_padding.left.size,
                Self::ZERO_PADDING,
                last_column_default_padding.top.size,
                last_column_default_padding.bottom.size,
            )));
    }

    fn terminal_width_for_column(
        &self,
        column_position: usize,
        total_table_width: usize,
    ) -> Result<usize, AnkTableError> {
        const DEFAULT_CONTENT_LENGTH: usize = 0;
        let terminal_width = terminal_width();
        let column_name_length = RowType::headers()[column_position].len();

        let max_content_length = self
            .rows
            .iter()
            .max_by_key(|row| RowType::fields(row)[column_position].len())
            .map(|row| RowType::fields(row)[column_position].len())
            .unwrap_or(DEFAULT_CONTENT_LENGTH);

        // the min length shall be the header column name length
        let column_width = max_content_length.max(column_name_length);

        let table_width_other_columns =
            total_table_width.checked_sub(column_width).ok_or_else(|| {
                AnkTableError(
                    "overflow when calculating table width for other columns.".to_string(),
                )
            })?;

        let is_reasonable_terminal_width = terminal_width
            .checked_sub(column_name_length)
            .ok_or_else(|| {
                AnkTableError("overflow when calculating reasonable terminal width.".to_string())
            })?
            >= table_width_other_columns;

        if is_reasonable_terminal_width {
            terminal_width
                .checked_sub(table_width_other_columns)
                .ok_or_else(|| {
                    AnkTableError("overflow when calculating remaining terminal width.".to_string())
                })
        } else {
            // no reasonable terminal width left, avoid breaking the column header name formatting
            Err(AnkTableError(
                "no reasonable terminal width available".to_string(),
            ))
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
    use super::AnkTable;
    use crate::cli_commands::workload_table_row::WorkloadTableRow;
    use common::objects::ExecutionState;

    // [utest->swdd~cli-shall-present-workloads-as-table~1]
    #[test]
    fn utest_create_default_table() {
        let table_rows = [WorkloadTableRow {
            name: "workload1".to_string(),
            agent: "agent1".to_string(),
            runtime: "podman".to_string(),
            execution_state: ExecutionState::running().to_string(),
            additional_info: "some long long additional info message".to_string(),
        }];

        let table = AnkTable::new(&table_rows);
        let table_output = table.create_default_table();
        let expected_table_output_newlines = 1;
        assert_eq!(
            table_output.matches('\n').count(),
            expected_table_output_newlines
        );
    }

    // [utest->swdd~cli-shall-present-workloads-as-table~1]
    #[test]
    fn utest_create_truncated_table_additional_info() {
        let table_rows = [WorkloadTableRow {
            name: "workload1".to_string(),
            agent: "agent1".to_string(),
            runtime: "podman".to_string(),
            execution_state: ExecutionState::running().to_string(),
            additional_info: "some long long additional info message".to_string(),
        }];

        let table = AnkTable::new(&table_rows);
        let table_output = table
            .table_with_truncated_column_to_remaining_terminal_width(
                WorkloadTableRow::ADDITIONAL_INFO_POS,
            )
            .unwrap();
        let expected_table_output_newlines = 1; // truncated additional info column with suffix '...'
        assert_eq!(
            table_output.matches('\n').count(),
            expected_table_output_newlines
        );

        let expected_additional_info_suffix = "...";
        assert_eq!(
            table_output
                .matches(expected_additional_info_suffix)
                .count(),
            1
        );
    }

    // [utest->swdd~cli-shall-present-workloads-as-table~1]
    #[test]
    fn utest_create_wrapped_table_additional_info() {
        let table_rows = [WorkloadTableRow {
            name: "workload1".to_string(),
            agent: "agent1".to_string(),
            runtime: "podman".to_string(),
            execution_state: ExecutionState::running().to_string(),
            additional_info: "some long long additional info message".to_string(),
        }];

        let table = AnkTable::new(&table_rows);
        let table_output = table
            .table_with_wrapped_column_to_remaining_terminal_width(
                WorkloadTableRow::ADDITIONAL_INFO_POS,
            )
            .unwrap_or_default();
        let expected_table_output_newlines = 2; // because of wrapping the additional info column
        assert_eq!(
            table_output.matches('\n').count(),
            expected_table_output_newlines
        );
    }

    // [utest->swdd~cli-shall-present-workloads-as-table~1]
    #[test]
    fn utest_terminal_width_for_additional_info_no_table_entries() {
        let empty_rows: [WorkloadTableRow; 0] = [];
        let table = AnkTable::new(&empty_rows);
        let table_width: usize = 70; // empty table but all header column names + paddings
        let column_position = WorkloadTableRow::ADDITIONAL_INFO_POS;
        let expected_terminal_width = Ok(25); // 80 (terminal width) - (70 - 15 (column name 'ADDITIONAL INFO')) = 25
        assert_eq!(
            table.terminal_width_for_column(column_position, table_width),
            expected_terminal_width
        );
    }

    // [utest->swdd~cli-shall-present-workloads-as-table~1]
    #[test]
    fn utest_terminal_width_for_additional_info_column_name_bigger_than_info_msg() {
        let table_rows = [WorkloadTableRow {
            name: "workload1".to_string(),
            agent: "agent1".to_string(),
            runtime: "podman".to_string(),
            execution_state: ExecutionState::running().to_string(),
            additional_info: "short".to_string(),
        }];

        let table = AnkTable::new(&table_rows);
        let table_width: usize = 70;
        let expected_terminal_width = Ok(25); // 80 (terminal width) - (70 - 15 (column name 'ADDITIONAL INFO')) = 25
        assert_eq!(
            table.terminal_width_for_column(WorkloadTableRow::ADDITIONAL_INFO_POS, table_width),
            expected_terminal_width
        );
    }

    // [utest->swdd~cli-shall-present-workloads-as-table~1]
    #[test]
    fn utest_terminal_width_for_additional_info_no_reasonable_terminal_width_left() {
        let table_rows = [WorkloadTableRow {
            name: "workload1".to_string(),
            agent: "agent1".to_string(),
            runtime: "podman".to_string(),
            execution_state: ExecutionState::running().to_string(),
            additional_info: "medium length message".to_string(),
        }];

        let table = AnkTable::new(&table_rows);
        let table_width: usize = 100; // table bigger than terminal width
        assert!(table
            .terminal_width_for_column(WorkloadTableRow::ADDITIONAL_INFO_POS, table_width)
            .is_err());
    }
}
