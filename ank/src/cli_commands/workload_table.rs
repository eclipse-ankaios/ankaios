#[cfg(not(test))]
use crate::log::terminal_width;

#[cfg(test)]
fn terminal_width() -> usize {
    80
}

use super::GetWorkloadTableDisplay;
use common::std_extensions::UnreachableOption;
use tabled::{
    settings::{object::Columns, Modify, Style, Width},
    Table, Tabled,
};
pub struct WorkloadTable<'a> {
    data: &'a [&'a GetWorkloadTableDisplay],
    table: Option<Table>,
}

impl<'a> WorkloadTable<'a> {
    pub fn new(data: &'a [&'a GetWorkloadTableDisplay]) -> Self {
        let mut table = Table::new(data);
        let basic_styled_table = table.with(Style::blank());
        Self {
            data,
            table: Some(basic_styled_table.to_owned()),
        }
    }

    pub fn create_default_table(&mut self) -> String {
        self.table_as_ref().unwrap_or_unreachable().to_string()
    }

    // [impl->swdd~cli-shall-present-workloads-as-table~1]
    pub fn create_table_truncated_additional_info(&mut self) -> Option<String> {
        let total_table_width: usize = self.table_as_mut()?.total_width();
        let additional_info_terminal_width =
            self.terminal_width_for_additional_info(total_table_width)?;

        const ADDITIONAL_INFO_SUFFIX: &str = "...";
        let suffix_length_additional_info = ADDITIONAL_INFO_SUFFIX.len();

        if additional_info_terminal_width > suffix_length_additional_info {
            self.truncate_table_column(
                additional_info_terminal_width,
                GetWorkloadTableDisplay::ADDITIONAL_INFO_POS,
                ADDITIONAL_INFO_SUFFIX,
                suffix_length_additional_info,
            );

            Some(self.table_as_mut()?.to_string())
        } else {
            None
        }
    }

    pub fn create_table_wrapped_additional_info(&mut self) -> Option<String> {
        let total_table_width: usize = self.table_as_mut()?.total_width();
        let additional_info_terminal_width =
            self.terminal_width_for_additional_info(total_table_width)?;

        self.wrap_table_column(
            additional_info_terminal_width,
            GetWorkloadTableDisplay::ADDITIONAL_INFO_POS,
        );

        Some(self.table_as_mut()?.to_string())
    }

    fn table_as_ref(&self) -> Option<&Table> {
        self.table.as_ref()
    }

    fn table_as_mut(&mut self) -> Option<&mut Table> {
        self.table.as_mut()
    }

    fn truncate_table_column(
        &mut self,
        remaining_terminal_width: usize,
        column_position: usize,
        suffix_additional_info: &str,
        suffix_length: usize,
    ) -> Option<()> {
        self.table_as_mut()?.with(
            Modify::new(Columns::single(column_position)).with(
                Width::truncate(remaining_terminal_width - suffix_length)
                    .suffix(suffix_additional_info),
            ),
        );
        Some(())
    }

    fn wrap_table_column(
        &mut self,
        remaining_terminal_width: usize,
        column_position: usize,
    ) -> Option<()> {
        self.table_as_mut()?.with(
            Modify::new(Columns::single(column_position))
                .with(Width::wrap(remaining_terminal_width)),
        );
        Some(())
    }

    fn terminal_width_for_additional_info(&self, total_table_width: usize) -> Option<usize> {
        let terminal_width = terminal_width();
        println!("terminal_width: {}", terminal_width);
        let column_name_length =
            GetWorkloadTableDisplay::headers()[GetWorkloadTableDisplay::ADDITIONAL_INFO_POS].len();
        let additional_info_width =
            if let Some(max_additional_info_length) = self.length_of_longest_additional_info() {
                if max_additional_info_length > column_name_length {
                    max_additional_info_length
                } else {
                    // Avoid wrapping the column name when additional info is shorter
                    column_name_length
                }
            } else {
                /* On empty table, the max length of the additional info is the column name itself
                to avoid wrapping the column name. */
                column_name_length
            };

        let table_width_except_last_column =
            total_table_width.checked_sub(additional_info_width)?;

        let is_reasonable_terminal_width = || {
            terminal_width >= column_name_length
                && (terminal_width - column_name_length) >= table_width_except_last_column
        };

        if is_reasonable_terminal_width() {
            Some(terminal_width - table_width_except_last_column)
        } else {
            None // no space left
        }
    }

    fn length_of_longest_additional_info(&self) -> Option<usize> {
        self.data
            .iter()
            .map(|table_info| table_info.additional_info.len())
            .max()
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
    use crate::cli_commands::GetWorkloadTableDisplay;
    use common::objects::ExecutionState;

    #[test]
    fn utest_create_default_table() {
        let table_row = GetWorkloadTableDisplay {
            name: "workload1".to_string(),
            agent: "agent1".to_string(),
            runtime: "podman".to_string(),
            execution_state: ExecutionState::running().to_string(),
            additional_info: "some long long additional info message".to_string(),
        };
        let data = [&table_row];

        let mut table = WorkloadTable::new(&data);
        let table_output = table.create_default_table();
        let expected_table_output_newlines = 1;
        assert_eq!(
            table_output.matches('\n').count(),
            expected_table_output_newlines
        );
    }

    #[test]
    fn utest_create_truncated_table_additional_info() {
        let table_row = GetWorkloadTableDisplay {
            name: "workload1".to_string(),
            agent: "agent1".to_string(),
            runtime: "podman".to_string(),
            execution_state: ExecutionState::running().to_string(),
            additional_info: "some long long additional info message".to_string(),
        };
        let data = [&table_row];

        let mut table = WorkloadTable::new(&data);
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

    #[test]
    fn utest_create_wrapped_table_additional_info() {
        let table_row = GetWorkloadTableDisplay {
            name: "workload1".to_string(),
            agent: "agent1".to_string(),
            runtime: "podman".to_string(),
            execution_state: ExecutionState::running().to_string(),
            additional_info: "some long long additional info message".to_string(),
        };
        let data = [&table_row];

        let mut table = WorkloadTable::new(&data);
        let table_output = table.create_table_wrapped_additional_info().unwrap();
        let expected_table_output_newlines = 2; // because of wrapping the additional info column
        assert_eq!(
            table_output.matches('\n').count(),
            expected_table_output_newlines
        );
    }

    #[test]
    fn utest_length_of_longest_additional_info_no_table_entries() {
        let table = WorkloadTable::new(&[]);
        assert_eq!(table.length_of_longest_additional_info(), None);
    }

    #[test]
    fn utest_length_of_longest_additional_info() {
        let additional_info = "some additional info message".to_string();
        let expected_additional_info_length = additional_info.len();
        let table_row = GetWorkloadTableDisplay {
            name: "workload1".to_string(),
            agent: "agent1".to_string(),
            runtime: "podman".to_string(),
            execution_state: ExecutionState::running().to_string(),
            additional_info,
        };
        let data = [&table_row];

        let table = WorkloadTable::new(&data);
        assert_eq!(
            table.length_of_longest_additional_info(),
            Some(expected_additional_info_length)
        );
    }

    #[test]
    fn utest_terminal_width_for_additional_info_no_table_entries() {
        let table = WorkloadTable::new(&[]);
        let table_width: usize = 70; // empty table but all header column names + paddings
        let expected_terminal_width = Some(25); // 80 (terminal width) - (70 - 15 (column name 'ADDITIONAL INFO')) = 25
        assert_eq!(
            table.terminal_width_for_additional_info(table_width),
            expected_terminal_width
        );
    }

    #[test]
    fn utest_terminal_width_for_additional_info_column_name_bigger_than_info_msg() {
        let table_row = GetWorkloadTableDisplay {
            name: "workload1".to_string(),
            agent: "agent1".to_string(),
            runtime: "podman".to_string(),
            execution_state: ExecutionState::running().to_string(),
            additional_info: "short".to_string(),
        };

        let data = [&table_row];

        let table = WorkloadTable::new(&data);
        let table_width: usize = 70;
        let expected_terminal_width = Some(25); // 80 (terminal width) - (70 - 15 (column name 'ADDITIONAL INFO')) = 25
        assert_eq!(
            table.terminal_width_for_additional_info(table_width),
            expected_terminal_width
        );
    }

    #[test]
    fn utest_terminal_width_for_additional_info_no_reasonable_terminal_width_left() {
        let table_row = GetWorkloadTableDisplay {
            name: "workload1".to_string(),
            agent: "agent1".to_string(),
            runtime: "podman".to_string(),
            execution_state: ExecutionState::running().to_string(),
            additional_info: "medium length message".to_string(),
        };

        let data = [&table_row];

        let table = WorkloadTable::new(&data);
        let table_width: usize = 100; // table bigger than terminal width
        assert!(table
            .terminal_width_for_additional_info(table_width)
            .is_none());
    }
}
