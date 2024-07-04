use tabled::Tabled;

pub trait MaxAdditionalInfo {
    fn length_of_longest_additional_info(&self) -> Option<usize>;
}

pub trait ColumnPosition {
    const FIRST_COLUMN_POS: usize;
    const EXECUTION_STATE_POS: usize;
    const ADDITIONAL_INFO_POS: usize;
}

#[derive(Debug, Tabled, Clone)]
#[tabled(rename_all = "UPPERCASE")]
pub struct WorkloadTableRow {
    #[tabled(rename = "WORKLOAD NAME")]
    pub name: String,
    pub agent: String,
    pub runtime: String,
    #[tabled(rename = "EXECUTION STATE")]
    pub execution_state: String,
    #[tabled(rename = "ADDITIONAL INFO")]
    pub additional_info: String,
}

impl MaxAdditionalInfo for Vec<WorkloadTableRow> {
    fn length_of_longest_additional_info(&self) -> Option<usize> {
        self.iter().map(|row| row.additional_info.len()).max()
    }
}

impl ColumnPosition for WorkloadTableRow {
    const FIRST_COLUMN_POS: usize = 0;
    const EXECUTION_STATE_POS: usize = 3;
    const ADDITIONAL_INFO_POS: usize = 4;
}

pub struct WorkloadTableRowWithSpinner<'a> {
    pub data: &'a WorkloadTableRow,
    pub spinner: &'a str,
}

impl MaxAdditionalInfo for Vec<WorkloadTableRowWithSpinner<'_>> {
    fn length_of_longest_additional_info(&self) -> Option<usize> {
        self.iter().map(|row| row.data.additional_info.len()).max()
    }
}

impl ColumnPosition for WorkloadTableRowWithSpinner<'_> {
    const FIRST_COLUMN_POS: usize = WorkloadTableRow::FIRST_COLUMN_POS;
    const EXECUTION_STATE_POS: usize = WorkloadTableRow::EXECUTION_STATE_POS;
    const ADDITIONAL_INFO_POS: usize = WorkloadTableRow::ADDITIONAL_INFO_POS;
}

impl<'a> Tabled for WorkloadTableRowWithSpinner<'a> {
    const LENGTH: usize = WorkloadTableRow::LENGTH;

    fn fields(&self) -> Vec<std::borrow::Cow<'_, str>> {
        let mut fields = self.data.fields();
        let execution_state = &mut fields[WorkloadTableRow::EXECUTION_STATE_POS];
        *(execution_state.to_mut()) = format!("{} {}", self.spinner, execution_state);

        fields
    }

    fn headers() -> Vec<std::borrow::Cow<'static, str>> {
        let mut headers = WorkloadTableRow::headers();
        *(headers[WorkloadTableRow::EXECUTION_STATE_POS].to_mut()) =
            format!("  {}", headers[WorkloadTableRow::EXECUTION_STATE_POS]);
        headers
    }
}

impl WorkloadTableRow {
    pub fn new(
        name: &str,
        agent: &str,
        runtime: &str,
        execution_state: &str,
        additional_info: &str,
    ) -> Self {
        WorkloadTableRow {
            name: name.to_string(),
            agent: agent.to_string(),
            runtime: runtime.to_string(),
            execution_state: execution_state.to_string(),
            additional_info: trim_and_replace_newlines(additional_info),
        }
    }

    pub fn set_additional_info(&mut self, new_additional_info: &str) {
        self.additional_info = trim_and_replace_newlines(new_additional_info);
    }
}

fn trim_and_replace_newlines(text: &str) -> String {
    text.trim().replace('\n', ", ")
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
    use tabled::Table;

    use super::{WorkloadTableRow, WorkloadTableRowWithSpinner};

    // [utest->swdd~cli-shall-present-workloads-as-table~1]
    #[test]
    fn utest_one_row_table() {
        let table_row = super::WorkloadTableRow {
            name: "workload".into(),
            agent: "agent".into(),
            runtime: "runtime".into(),
            execution_state: "execution_state".into(),
            additional_info: "additional_info".into(),
        };
        let table_rows_with_spinner = vec![WorkloadTableRowWithSpinner {
            data: &table_row,
            spinner: "/",
        }];
        let mut table = Table::new(table_rows_with_spinner);
        let expected_table = " WORKLOAD NAME   AGENT   RUNTIME     EXECUTION STATE   ADDITIONAL INFO \n workload        agent   runtime   / execution_state   additional_info ";
        assert_eq!(
            table.with(tabled::settings::Style::blank()).to_string(),
            expected_table
        );
    }

    // [utest->swdd~cli-shall-present-workloads-as-table~1]
    #[test]
    fn utest_additional_info_msg_without_new_lines() {
        let additional_info_msg = "some error with\nsome\nnewlines";
        let mut get_workloads_table_display = WorkloadTableRow::new(
            "workload1",
            "agent1",
            "runtime_x",
            "running",
            additional_info_msg,
        );

        assert_eq!(
            get_workloads_table_display.additional_info,
            "some error with, some, newlines"
        );

        let updated_additional_info_msg = "different error with\na new line";
        get_workloads_table_display.set_additional_info(updated_additional_info_msg);

        assert_eq!(
            get_workloads_table_display.additional_info,
            "different error with, a new line"
        );
    }
}
