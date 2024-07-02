use std::{
    collections::{HashMap, HashSet},
    fmt::{self, Display},
};

use common::objects::WorkloadInstanceName;

use crate::{cli_commands::workload_table_row::WorkloadTableRowWithSpinner, output_debug};

use super::workload_table::WorkloadTable;
use super::{wait_list::WaitListDisplayTrait, workload_table_row::WorkloadTableRow};

pub(crate) const COMPLETED_SYMBOL: &str = " ";
const SPINNER_SYMBOLS: [&str; 4] = ["|", "/", "-", "\\"];

pub struct WaitListDisplay {
    pub data: HashMap<WorkloadInstanceName, WorkloadTableRow>,
    pub not_completed: HashSet<WorkloadInstanceName>,
    pub spinner: Spinner,
}

impl Display for WaitListDisplay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let current_spinner = self.spinner.to_string();
        let mut data: Vec<_> = self
            .data
            .iter()
            .map(|(workload_name, table_entry)| {
                let update_state_symbol = if self.not_completed.contains(workload_name) {
                    &current_spinner
                } else {
                    COMPLETED_SYMBOL
                };
                WorkloadTableRowWithSpinner {
                    data: table_entry,
                    spinner: update_state_symbol,
                }
            })
            .collect();
        data.sort_by_key(|x| &x.data.name);

        let workload_infos: Vec<&WorkloadTableRow> = data.iter().map(|x| x.data).collect();

        // [impl->swdd~cli-shall-present-workloads-as-table~1]
        let mut workload_table_infos = WorkloadTable::new(&workload_infos);

        let table_output = workload_table_infos
            .create_table_truncated_additional_info()
            .unwrap_or_else(|| {
                output_debug!(
                    "Failed to create truncated table output. Continue with default table layout."
                );

                workload_table_infos.create_default_table()
            });

        write!(f, "{}", table_output)
    }
}

impl WaitListDisplayTrait for WaitListDisplay {
    fn update(&mut self, workload_state: &common::objects::WorkloadState) {
        if let Some(entry) = self.data.get_mut(&workload_state.instance_name) {
            entry.execution_state = workload_state.execution_state.state.to_string();
            entry.set_additional_info(&workload_state.execution_state.additional_info);
        }
    }

    fn set_complete(&mut self, workload: &WorkloadInstanceName) {
        self.not_completed.remove(workload);
    }

    fn step_spinner(&mut self) {
        self.spinner.step();
    }
}

#[derive(Default)]
pub struct Spinner {
    pos: usize,
}

impl Spinner {
    pub fn step(&mut self) {
        self.pos = (self.pos + 1) % SPINNER_SYMBOLS.len();
    }
}

impl Display for Spinner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", SPINNER_SYMBOLS[self.pos])
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

    use std::collections::{HashMap, HashSet};

    use common::objects::{ExecutionState, WorkloadInstanceName, WorkloadState};

    use crate::cli_commands::{
        wait_list::WaitListDisplayTrait, workload_table_row::WorkloadTableRow,
    };

    use super::WaitListDisplay;

    #[test]
    fn update_table() {
        let workload_instance_name = WorkloadInstanceName::builder()
            .agent_name("agent")
            .config(&String::from("runtime"))
            .workload_name("workload")
            .build();
        let mut wait_list_display = WaitListDisplay {
            data: HashMap::from([(
                workload_instance_name.clone(),
                WorkloadTableRow {
                    name: "workload".into(),
                    agent: "agent".into(),
                    runtime: "runtime".into(),
                    execution_state: "execution_state".into(),
                    additional_info: "additional_info".into(),
                },
            )]),
            not_completed: HashSet::from([workload_instance_name.clone()]),
            spinner: Default::default(),
        };

        assert_eq!(
            wait_list_display
                .data
                .get(&workload_instance_name)
                .unwrap()
                .execution_state,
            "execution_state"
        );
        wait_list_display.update(&WorkloadState {
            instance_name: workload_instance_name.clone(),
            execution_state: ExecutionState::succeeded(),
        });
        assert_eq!(
            wait_list_display
                .data
                .get(&workload_instance_name)
                .unwrap()
                .execution_state,
            "Succeeded(Ok)"
        );
    }
}
