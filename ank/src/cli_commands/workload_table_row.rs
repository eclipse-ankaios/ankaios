use tabled::Tabled;

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

pub struct WorkloadTableRowWithSpinner<'a> {
    pub data: &'a WorkloadTableRow,
    pub spinner: &'a str,
}

impl WorkloadTableRow {
    const EXECUTION_STATE_POS: usize = 3;
}

impl<'a> Tabled for WorkloadTableRowWithSpinner<'a> {
    const LENGTH: usize = WorkloadTableRow::LENGTH;

    fn fields(&self) -> Vec<std::borrow::Cow<'_, str>> {
        let mut fields = self.data.fields();
        *(fields[WorkloadTableRow::EXECUTION_STATE_POS].to_mut()) = format!(
            "{} {}",
            self.spinner,
            fields[WorkloadTableRow::EXECUTION_STATE_POS]
        );
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
        name: impl Into<String>,
        agent: impl Into<String>,
        runtime: impl Into<String>,
        execution_state: impl Into<String>,
        additional_info: impl Into<String>,
    ) -> Self {
        WorkloadTableRow {
            name: name.into(),
            agent: agent.into(),
            runtime: runtime.into(),
            execution_state: execution_state.into(),
            additional_info: additional_info.into(),
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
    use tabled::Table;

    use super::WorkloadTableRowWithSpinner;

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
}
