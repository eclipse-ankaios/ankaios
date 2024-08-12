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
pub struct TableBuilder<RowType> {
    rows: Vec<RowType>,
    table: Table,
    fallback_to_default: bool,
    error: bool,
}

impl<RowType> TableBuilder<RowType>
where
    RowType: Tabled,
{
    const TRUNCATED_COLUMN_SUFFIX: &'static str = "...";

    pub fn new(rows: Vec<RowType>) -> Self {
        let table = Table::new(&rows);
        Self {
            rows,
            table,
            fallback_to_default: false,
            error: false,
        }
    }

    pub fn style_blank(mut self) -> Self {
        self.table.with(Style::blank());
        self
    }

    pub fn disable_surrounding_padding(mut self) -> Self {
        const FIRST_COLUMN_POS: usize = 0;
        const ZERO_PADDING: usize = 0;

        let column_count = self.table.count_columns();
        let last_column_pos = column_count - 1;

        let first_column_default_padding = self
            .table
            .get_config()
            .get_padding(tabled::grid::config::Entity::Column(FIRST_COLUMN_POS));

        let last_column_default_padding = self
            .table
            .get_config()
            .get_padding(tabled::grid::config::Entity::Column(last_column_pos));

        /* Set the left padding of the first and the right padding of the last column to zero
        to align the table content to the full terminal width for better output quality. */
        self.table
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
        self
    }

    pub fn wrap_column_to_remaining_terminal_width(mut self, column_position: usize) -> Self {
        let total_table_width: usize = self.table.total_width();
        if let Some(available_column_width) =
            self.terminal_width_for_column(column_position, total_table_width)
        {
            self.table.with(
                Modify::new(Columns::single(column_position))
                    .with(Width::wrap(available_column_width)),
            );
        } else {
            self.error = true;
        }
        self
    }

    pub fn truncate_column_to_remaining_terminal_width(mut self, column_position: usize) -> Self {
        let total_table_width: usize = self.table.total_width();
        if let Some(available_column_width) =
            self.terminal_width_for_column(column_position, total_table_width)
        {
            self.table
                .with(Modify::new(Columns::single(column_position)).with(
                    Width::truncate(available_column_width).suffix(Self::TRUNCATED_COLUMN_SUFFIX),
                ));
        } else {
            self.error = true;
        }
        self
    }

    pub fn fallback_to_default_table(mut self) -> Self {
        self.fallback_to_default = true;
        self
    }

    pub fn create_default_table(mut self) -> String {
        self.table = Table::new(&self.rows);
        self.table.with(Style::blank());
        self = self.disable_surrounding_padding();
        self.table.to_string()
    }

    pub fn build(self) -> String {
        if self.error && self.fallback_to_default {
            self.create_default_table()
        } else {
            self.table.to_string()
        }
    }

    fn terminal_width_for_column(
        &self,
        column_position: usize,
        total_table_width: usize,
    ) -> Option<usize> {
        let terminal_width = terminal_width();
        let column_name_length = RowType::headers()[column_position].len();

        let max_content_size = self
            .rows
            .iter()
            .max_by_key(|row| RowType::fields(row)[column_position].len())
            .map(|row| RowType::fields(row)[column_position].len())
            .unwrap_or(0);

        // the min length shall be the header column name length
        let column_width = max_content_size.max(column_name_length);

        let table_width_except_last_column = total_table_width.checked_sub(column_width)?;

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
    use super::TableBuilder;
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

        let table = TableBuilder::new(table_rows);
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

        let table = TableBuilder::new(table_rows);
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

        let table = TableBuilder::new(table_rows);
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
        let table = TableBuilder::new(empty_table);
        let table_width: usize = 70; // empty table but all header column names + paddings
        let expected_terminal_width = Some(25); // 80 (terminal width) - (70 - 15 (column name 'ADDITIONAL INFO')) = 25
        assert_eq!(
            table.terminal_width_for_column(table_width),
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

        let table = TableBuilder::new(table_rows);
        let table_width: usize = 70;
        let expected_terminal_width = Some(25); // 80 (terminal width) - (70 - 15 (column name 'ADDITIONAL INFO')) = 25
        assert_eq!(
            table.terminal_width_for_column(table_width),
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

        let table = TableBuilder::new(table_rows);
        let table_width: usize = 100; // table bigger than terminal width
        assert!(table.terminal_width_for_column(table_width).is_none());
    }
}
