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

use super::workload_table_row::{ColumnPosition, MaxAdditionalInfo};
use tabled::{
    settings::{object::Columns, Modify, Padding, Style, Width},
    Table, Tabled,
};
pub struct WorkloadTable<RowType> {
    rows: Vec<RowType>,
}

impl<RowType> WorkloadTable<RowType>
where
    RowType: Tabled + ColumnPosition,
    Vec<RowType>: MaxAdditionalInfo,
{
    const ADDITIONAL_INFO_SUFFIX: &'static str = "...";

    pub fn new(rows: Vec<RowType>) -> Self {
        Self { rows }
    }

    // [impl->swdd~cli-shall-present-workloads-as-table~1]
    pub fn create_default_table(&self) -> String {
        let default_table = Self::default_table(&self.rows);
        default_table.to_string()
    }

    // [impl->swdd~cli-shall-present-workloads-as-table~1]
    pub fn create_table_truncated_additional_info(&self) -> Option<String> {
        let default_table = Self::default_table(&self.rows);

        let total_table_width: usize = default_table.total_width();
        let available_additional_info_width =
            self.terminal_width_for_additional_info(total_table_width)?;

        let truncated_table = Self::truncate_column_of_table(
            RowType::ADDITIONAL_INFO_POS,
            default_table,
            available_additional_info_width,
            Self::ADDITIONAL_INFO_SUFFIX,
        );

        Some(truncated_table.to_string())
    }

    // [impl->swdd~cli-shall-present-workloads-as-table~1]
    pub fn create_table_wrapped_additional_info(&self) -> Option<String> {
        let default_table = Self::default_table(&self.rows);

        let total_table_width: usize = default_table.total_width();
        let available_additional_info_width =
            self.terminal_width_for_additional_info(total_table_width)?;

        let wrapped_table = Self::wrap_column_of_table(
            RowType::ADDITIONAL_INFO_POS,
            default_table,
            available_additional_info_width,
        );

        Some(wrapped_table.to_string())
    }

    fn default_table(rows: &Vec<RowType>) -> Table {
        let mut table = Table::new(rows);
        let basic_table = table.with(Style::blank()).to_owned();

        Self::set_custom_table_padding(basic_table)
    }

    fn set_custom_table_padding(mut table: Table) -> Table {
        let first_column_default_padding =
            table
                .get_config()
                .get_padding(tabled::grid::config::Entity::Column(
                    RowType::FIRST_COLUMN_POS,
                ));

        let last_column_default_padding =
            table
                .get_config()
                .get_padding(tabled::grid::config::Entity::Column(
                    RowType::ADDITIONAL_INFO_POS,
                ));

        /* Set the left padding of the first and the right padding of the last column to zero
        to align the table content to the full terminal width for better output quality. */
        const ZERO_PADDING: usize = 0;
        table
            .with(Modify::new(Columns::first()).with(Padding::new(
                ZERO_PADDING,
                first_column_default_padding.right.size,
                first_column_default_padding.top.size,
                first_column_default_padding.bottom.size,
            )))
            .with(Modify::new(Columns::last()).with(Padding::new(
                last_column_default_padding.left.size,
                ZERO_PADDING,
                last_column_default_padding.top.size,
                last_column_default_padding.bottom.size,
            )));
        table
    }

    fn truncate_column_of_table(
        column_position: usize,
        mut table: Table,
        new_column_width: usize,
        suffix_additional_info: &str,
    ) -> Table {
        table.with(
            Modify::new(Columns::single(column_position))
                .with(Width::truncate(new_column_width).suffix(suffix_additional_info)),
        );
        table
    }

    fn wrap_column_of_table(
        column_position: usize,
        mut table: Table,
        new_column_width: usize,
    ) -> Table {
        table.with(
            Modify::new(Columns::single(column_position)).with(Width::wrap(new_column_width)),
        );
        table
    }

    fn terminal_width_for_additional_info(&self, total_table_width: usize) -> Option<usize> {
        let terminal_width = terminal_width();
        let column_name_length = RowType::headers()[RowType::ADDITIONAL_INFO_POS].len();

        let additional_info_width = self
            .rows
            .length_of_longest_additional_info()
            .unwrap_or(0) // to rows => length is 0
            .max(column_name_length); // the min length shall be the header column name length

        let table_width_except_last_column =
            total_table_width.checked_sub(additional_info_width)?;

        let is_reasonable_terminal_width =
            terminal_width.checked_sub(column_name_length)? >= table_width_except_last_column;

        if is_reasonable_terminal_width {
            terminal_width.checked_sub(table_width_except_last_column)
        } else {
            None // no reasonable terminal width left, avoid breaking the column header name formatting
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
    use super::WorkloadTable;
    use crate::cli_commands::WorkloadTableRow;
    use common::objects::ExecutionState;

    // [utest->swdd~cli-shall-present-workloads-as-table~1]
    #[test]
    fn utest_create_default_table() {
        let row: WorkloadTableRow = WorkloadTableRow {
            name: "workload1".to_string(),
            agent: "agent1".to_string(),
            runtime: "podman".to_string(),
            execution_state: ExecutionState::running().to_string(),
            additional_info: "some long long additional info message".to_string(),
        };

        let table_rows = vec![row];

        let table = WorkloadTable::new(table_rows);
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
        let row = WorkloadTableRow {
            name: "workload1".to_string(),
            agent: "agent1".to_string(),
            runtime: "podman".to_string(),
            execution_state: ExecutionState::running().to_string(),
            additional_info: "some long long additional info message".to_string(),
        };

        let table_rows = vec![row];

        let table = WorkloadTable::new(table_rows);
        let table_output = table.create_table_truncated_additional_info().unwrap();
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
        let row = WorkloadTableRow {
            name: "workload1".to_string(),
            agent: "agent1".to_string(),
            runtime: "podman".to_string(),
            execution_state: ExecutionState::running().to_string(),
            additional_info: "some long long additional info message".to_string(),
        };

        let table_rows = vec![row];

        let table = WorkloadTable::new(table_rows);
        let table_output = table
            .create_table_wrapped_additional_info()
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
        let empty_table: Vec<WorkloadTableRow> = Vec::new();
        let table = WorkloadTable::new(empty_table);
        let table_width: usize = 70; // empty table but all header column names + paddings
        let expected_terminal_width = Some(25); // 80 (terminal width) - (70 - 15 (column name 'ADDITIONAL INFO')) = 25
        assert_eq!(
            table.terminal_width_for_additional_info(table_width),
            expected_terminal_width
        );
    }

    // [utest->swdd~cli-shall-present-workloads-as-table~1]
    #[test]
    fn utest_terminal_width_for_additional_info_column_name_bigger_than_info_msg() {
        let row = WorkloadTableRow {
            name: "workload1".to_string(),
            agent: "agent1".to_string(),
            runtime: "podman".to_string(),
            execution_state: ExecutionState::running().to_string(),
            additional_info: "short".to_string(),
        };

        let table_rows = vec![row];

        let table = WorkloadTable::new(table_rows);
        let table_width: usize = 70;
        let expected_terminal_width = Some(25); // 80 (terminal width) - (70 - 15 (column name 'ADDITIONAL INFO')) = 25
        assert_eq!(
            table.terminal_width_for_additional_info(table_width),
            expected_terminal_width
        );
    }

    // [utest->swdd~cli-shall-present-workloads-as-table~1]
    #[test]
    fn utest_terminal_width_for_additional_info_no_reasonable_terminal_width_left() {
        let row = WorkloadTableRow {
            name: "workload1".to_string(),
            agent: "agent1".to_string(),
            runtime: "podman".to_string(),
            execution_state: ExecutionState::running().to_string(),
            additional_info: "medium length message".to_string(),
        };

        let table_rows = vec![row];

        let table = WorkloadTable::new(table_rows);
        let table_width: usize = 100; // table bigger than terminal width
        assert!(table
            .terminal_width_for_additional_info(table_width)
            .is_none());
    }
}
