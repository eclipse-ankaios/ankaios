use std::collections::{BTreeMap, BTreeSet};
use tabled::Tabled;
use super::cli_table::CliTable;
use common::objects::WorkloadInstanceName;

#[derive(Clone, Debug)]
pub enum DryRunAction {
    NoChange,
    Create,
    Recreate,
    Delete,
}

impl std::fmt::Display for DryRunAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            DryRunAction::NoChange => "no change",
            DryRunAction::Create   => "create",
            DryRunAction::Recreate => "recreate",
            DryRunAction::Delete   => "delete",
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
    #[tabled(rename = "DETAILS")]
    pub details: String,
}

impl DryRunPlanRow {
    pub const DETAILS_POS: usize = 2;
}

pub fn workloads_from_masks(masks: &[String]) -> BTreeSet<String> {
    const PREFIX: &str = "desiredState.workloads.";
    masks.iter()
        .filter_map(|m| m.strip_prefix(PREFIX))
        .map(|rest| rest.split('.').next().unwrap_or(rest).to_string())
        .collect()
}

pub fn counts_by_workload_agent(instance_names: &[String]) -> BTreeMap<(String, String), usize> {
    let mut map: BTreeMap<(String, String), usize> = BTreeMap::new();
    for s in instance_names {
        if let Ok(win) = WorkloadInstanceName::try_from(s.clone()) {
            let key = (win.workload_name().to_string(), win.agent_name().to_string());
            *map.entry(key).or_default() += 1;
        } else {
            let wl = s.split('.').next().unwrap_or(s).to_string();
            *map.entry((wl, String::from("-"))).or_default() += 1;
        }
    }
    map
}

pub fn build_dry_run_rows(
    filter_masks: &[String],
    added: &[String],
    deleted: &[String],
) -> Vec<DryRunPlanRow> {
    let target_wls = workloads_from_masks(filter_masks);
    let add_counts = counts_by_workload_agent(added);
    let del_counts = counts_by_workload_agent(deleted);

    let mut changed_keys: BTreeSet<(String, String)> =
        add_counts.keys().cloned().collect();
    changed_keys.extend(del_counts.keys().cloned());

    let mut rows: Vec<DryRunPlanRow> = Vec::new();

    for (wl, agent) in changed_keys {
        let added_count = *add_counts.get(&(wl.clone(), agent.clone())).unwrap_or(&0);
        let deleted_count = *del_counts.get(&(wl.clone(), agent.clone())).unwrap_or(&0);

        let action = match (added_count > 0, deleted_count > 0) {
            (true,  true ) => DryRunAction::Recreate,
            (true,  false) => DryRunAction::Create,
            (false, true ) => DryRunAction::Delete,
            (false, false) => DryRunAction::NoChange,
        };

        let details = match action {
            DryRunAction::Recreate => format!("delete: {deleted_count}, create: {added_count}"),
            DryRunAction::Create   => format!("instances: {added_count}"),
            DryRunAction::Delete   => format!("instances: {deleted_count}"),
            DryRunAction::NoChange => "-".into(),
        };

        rows.push(DryRunPlanRow {
            workload: wl,
            action: action.to_string(),
            details,
        });
    }

    let changed_wls: BTreeSet<String> = rows.iter().map(|r| r.workload.clone()).collect();
    for wl in target_wls {
        if !changed_wls.contains(&wl) {
            rows.push(DryRunPlanRow {
                workload: wl,
                action: DryRunAction::NoChange.to_string(),
                details: "-".into(),
            });
        }
    }

    rows
}


pub fn render_dry_run_table(rows: &[DryRunPlanRow]) -> String {
    let refs: Vec<&DryRunPlanRow> = rows.iter().collect();
    CliTable::new(&refs)
        .table_with_wrapped_column_to_remaining_terminal_width(DryRunPlanRow::DETAILS_POS)
        .unwrap_or_else(|_| CliTable::new(&refs).create_default_table())
}
