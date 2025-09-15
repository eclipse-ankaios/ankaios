use std::collections::{BTreeSet};
use tabled::Tabled;
use super::cli_table::CliTable;


#[derive(Clone, Debug)]
pub enum DryRunAction {
    Add,
    Update,
    Delete,
}

impl std::fmt::Display for DryRunAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            DryRunAction::Add    => "added",
            DryRunAction::Update => "updated",
            DryRunAction::Delete => "deleted",
        };
        write!(f, "{s}")
    }
}

#[derive(Clone, Debug, Tabled)]
#[tabled(rename_all = "UPPERCASE")]
pub struct DryRunPlanRow {
    #[tabled(rename = "WORKLOAD")]
    pub workload: String,
    #[tabled(rename = "ACTION")]
    pub action: String,
}

impl DryRunPlanRow {
    pub const ACTION_POSITION: usize = 1;
}
pub fn build_dry_run_rows(
    added: &[String],
    deleted: &[String],
) -> Vec<DryRunPlanRow> {
    let added_wls: BTreeSet<String> = added
        .iter()
        .map(|s| s.split('.').next().unwrap_or(s).to_string())
        .collect();

    let deleted_wls: BTreeSet<String> = deleted
        .iter()
        .map(|s| s.split('.').next().unwrap_or(s).to_string())
        .collect();

    let mut rows = vec![];

    for workload in added_wls.intersection(&deleted_wls) {
        rows.push(DryRunPlanRow {
            workload: workload.clone(),
            action: DryRunAction::Update.to_string(),
        });
    }

    for workload in added_wls.difference(&deleted_wls) {
        rows.push(DryRunPlanRow {
            workload: workload.clone(),
            action: DryRunAction::Add.to_string(),
        });
    }

    for workload in deleted_wls.difference(&added_wls) {
        rows.push(DryRunPlanRow {
            workload: workload.clone(),
            action: DryRunAction::Delete.to_string(),
        });
    }

    rows.sort_by(|a, b| a.workload.cmp(&b.workload));
    rows
}

pub fn render_dry_run_table(rows: &[DryRunPlanRow]) -> String {
    let refs: Vec<&DryRunPlanRow> = rows.iter().collect();
    CliTable::new(&refs)
        .table_with_truncated_column_to_remaining_terminal_width(DryRunPlanRow::ACTION_POSITION)
        .unwrap_or_else(|_| CliTable::new(&refs).create_default_table())
}
