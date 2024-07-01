use std::{
    collections::{HashMap, HashSet},
    fmt::{self, Display},
};

use common::objects::WorkloadInstanceName;
use tabled::Table;

use crate::cli_commands::workload_table_row::WorkloadTableRowWithSpinner;

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

        // [impl->swdd~cli-shall-present-workloads-as-table~1]
        write!(
            f,
            "{}",
            Table::new(data).with(tabled::settings::Style::blank())
        )
    }
}

impl WaitListDisplayTrait for WaitListDisplay {
    fn update(&mut self, workload_state: &common::objects::WorkloadState) {
        if let Some(entry) = self.data.get_mut(&workload_state.instance_name) {
            entry.execution_state = workload_state.execution_state.state.to_string();
            entry
                .additional_info
                .clone_from(&workload_state.execution_state.additional_info);
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
