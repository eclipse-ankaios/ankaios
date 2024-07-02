#[cfg(not(test))]
use crate::log::terminal_width;

#[cfg(test)]
fn terminal_width() -> usize {
    80
}

use super::workload_table_row::MaxAdditionalInfo;
use super::WorkloadTableRow;
use tabled::{
    settings::{object::Columns, Modify, Padding, Style, Width},
    Table, Tabled,
};
pub struct WorkloadTable {
    table: Table,
    length_of_longest_additional_info: Option<usize>,
}

impl WorkloadTable {
    const ADDITIONAL_INFO_SUFFIX: &'static str = "...";

    pub fn new<RowType>(rows: Vec<RowType>) -> Self
    where
        RowType: Tabled,
        Vec<RowType>: MaxAdditionalInfo,
    {
        let length_of_longest_additional_info = rows.length_of_longest_additional_info();
        let mut table = Table::new(rows);
        let basic_table = table.with(Style::blank()).to_owned();

        let custom_table = Self::set_custom_table_padding(basic_table);

        Self {
            table: custom_table.to_owned(),
            length_of_longest_additional_info,
        }
    }

    // [impl->swdd~cli-shall-present-workloads-as-table~1]
    pub fn create_default_table(&mut self) -> String {
        self.table.to_string()
    }

    // [impl->swdd~cli-shall-present-workloads-as-table~1]
    pub fn create_table_truncated_additional_info(&mut self) -> Option<String> {
        let total_table_width: usize = self.table.total_width();
        let additional_info_terminal_width =
            self.terminal_width_for_additional_info(total_table_width)?;

        self.truncate_table_column(
            WorkloadTableRow::ADDITIONAL_INFO_POS,
            additional_info_terminal_width,
            Self::ADDITIONAL_INFO_SUFFIX,
        );

        Some(self.table.to_string())
    }

    // [impl->swdd~cli-shall-present-workloads-as-table~1]
    pub fn create_table_wrapped_additional_info(&mut self) -> Option<String> {
        let total_table_width: usize = self.table.total_width();
        let additional_info_terminal_width =
            self.terminal_width_for_additional_info(total_table_width)?;

        self.wrap_table_column(
            WorkloadTableRow::ADDITIONAL_INFO_POS,
            additional_info_terminal_width,
        );

        Some(self.table.to_string())
    }

    fn set_custom_table_padding(mut table: Table) -> Table {
        let first_column_default_padding =
            table
                .get_config()
                .get_padding(tabled::grid::config::Entity::Column(
                    WorkloadTableRow::FIRST_COLUMN_POS,
                ));

        let last_column_default_padding =
            table
                .get_config()
                .get_padding(tabled::grid::config::Entity::Column(
                    WorkloadTableRow::ADDITIONAL_INFO_POS,
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

    fn truncate_table_column(
        &mut self,
        column_position: usize,
        remaining_terminal_width: usize,
        suffix_additional_info: &str,
    ) {
        self.table.with(
            Modify::new(Columns::single(column_position))
                .with(Width::truncate(remaining_terminal_width).suffix(suffix_additional_info)),
        );
    }

    fn wrap_table_column(&mut self, column_position: usize, remaining_terminal_width: usize) {
        self.table.with(
            Modify::new(Columns::single(column_position))
                .with(Width::wrap(remaining_terminal_width)),
        );
    }

    fn terminal_width_for_additional_info(&self, total_table_width: usize) -> Option<usize> {
        let terminal_width = terminal_width();
        let column_name_length =
            WorkloadTableRow::headers()[WorkloadTableRow::ADDITIONAL_INFO_POS].len();
        let additional_info_width =
            if let Some(max_additional_info_length) = self.length_of_longest_additional_info {
                if max_additional_info_length > column_name_length {
                    max_additional_info_length
                } else {
                    // Avoid messing up the column name when additional info is shorter
                    column_name_length
                }
            } else {
                /* On empty table, the max length of the additional info is the column name itself
                to avoid messing up the column name in the output. */
                column_name_length
            };

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

        let mut table = WorkloadTable::new(table_rows);
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

        let mut table = WorkloadTable::new(table_rows);
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

        let mut table = WorkloadTable::new(table_rows);
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
