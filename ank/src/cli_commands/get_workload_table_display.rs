use tabled::Tabled;

#[derive(Debug, Tabled, Clone)]
#[tabled(rename_all = "UPPERCASE")]
pub struct GetWorkloadTableDisplay {
    #[tabled(rename = "WORKLOAD NAME")]
    pub name: String,
    pub agent: String,
    pub runtime: String,
    #[tabled(rename = "EXECUTION STATE")]
    pub execution_state: String,
    #[tabled(rename = "ADDITIONAL INFO")]
    pub additional_info: String,
}

pub struct GetWorkloadTableDisplayWithSpinner<'a> {
    pub data: &'a GetWorkloadTableDisplay,
    pub spinner: &'a str,
}

impl GetWorkloadTableDisplay {
    const EXECUTION_STATE_POS: usize = 3;
}

impl<'a> Tabled for GetWorkloadTableDisplayWithSpinner<'a> {
    const LENGTH: usize = GetWorkloadTableDisplay::LENGTH;

    fn fields(&self) -> Vec<std::borrow::Cow<'_, str>> {
        let mut fields = self.data.fields();
        *(fields[GetWorkloadTableDisplay::EXECUTION_STATE_POS].to_mut()) = format!(
            "{} {}",
            self.spinner,
            fields[GetWorkloadTableDisplay::EXECUTION_STATE_POS]
        );
        fields
    }

    fn headers() -> Vec<std::borrow::Cow<'static, str>> {
        let mut headers = GetWorkloadTableDisplay::headers();
        *(headers[GetWorkloadTableDisplay::EXECUTION_STATE_POS].to_mut()) = format!(
            "  {}",
            headers[GetWorkloadTableDisplay::EXECUTION_STATE_POS]
        );
        headers
    }
}

impl GetWorkloadTableDisplay {
    pub fn new(
        name: &str,
        agent: &str,
        runtime: &str,
        execution_state: &str,
        additional_info: &str,
    ) -> Self {
        GetWorkloadTableDisplay {
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
