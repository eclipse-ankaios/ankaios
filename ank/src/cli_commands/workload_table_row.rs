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
            additional_info: additional_info.to_string(),
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
mod tests {}
