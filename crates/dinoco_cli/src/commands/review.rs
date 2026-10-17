//! Reviewing a migration plan: the decisions it needs (inferred renames,
//! replacements for removed enum values, approval), how it is presented, and
//! how each decision is taken from an option, a prompt, or reported as missing.

use std::collections::{BTreeMap, BTreeSet};

use dinoco_engine::MigrationColumnType;

use crate::db::{CliDatabase, DatabaseSchema};
use crate::error::CliError;
use crate::prompt::{self, Choice};
use crate::sql::{EnumValueReplacement, InferredRename, MigrationPlan, MigrationStep, PlanOptions};
use crate::ui::{self, Change};

/// Options of `dinoco migrate generate`.
#[derive(Debug, Clone, Default)]
pub struct GenerateOptions {
    pub workspace: Option<String>,
    /// Workspace whose saved schema is applied instead of dinoco/schema.dinoco.
    pub schema_from: Option<String>,
    pub yes: bool,
    pub allow_destructive: bool,
    pub create_only: bool,
    pub accept_renames: bool,
    pub renames: Vec<String>,
    pub rejected_renames: Vec<String>,
    pub enum_replacements: Vec<String>,
    pub repair_drift: bool,
    pub adopt_baseline: bool,
}

/// The decisions `migrate generate` can take without asking, from options
/// and from the environment variables earlier releases used.
#[derive(Debug, Clone)]
pub(crate) struct Decisions {
    pub create_only: bool,
    yes: bool,
    allow_destructive: bool,
    accept_renames: bool,
    renames: BTreeSet<InferredRename>,
    rejected_renames: BTreeSet<InferredRename>,
    enum_replacements: BTreeMap<(String, String), String>,
    repair_drift: bool,
    adopt_baseline: bool,
    /// `DINOCO_CLI_CONFIRM_MIGRATION=true`: approves the plan as shown,
    /// destructive changes and inferred renames included.
    legacy_approve: bool,
    /// `DINOCO_CLI_CONFIRM_DESTRUCTIVE=true`: approves destructive plans as
    /// shown, inferred renames included.
    legacy_destructive: bool,
}

fn env_flag(key: &str) -> bool {
    std::env::var(key).ok().as_deref() == Some("true")
}

impl Decisions {
    pub fn new(options: &GenerateOptions) -> Result<Self, CliError> {
        let legacy_drift = env_flag("DINOCO_CLI_CONFIRM_DRIFT");
        Ok(Self {
            create_only: options.create_only,
            yes: options.yes,
            allow_destructive: options.allow_destructive,
            accept_renames: options.accept_renames,
            renames: options
                .renames
                .iter()
                .map(|mapping| parse_rename(mapping, "--rename"))
                .collect::<Result<_, _>>()?,
            rejected_renames: options
                .rejected_renames
                .iter()
                .map(|mapping| parse_rename(mapping, "--no-rename"))
                .collect::<Result<_, _>>()?,
            enum_replacements: options
                .enum_replacements
                .iter()
                .map(|mapping| parse_enum_replacement(mapping))
                .collect::<Result<_, _>>()?,
            repair_drift: options.repair_drift || legacy_drift,
            adopt_baseline: options.adopt_baseline || legacy_drift,
            legacy_approve: env_flag("DINOCO_CLI_CONFIRM_MIGRATION"),
            legacy_destructive: env_flag("DINOCO_CLI_CONFIRM_DESTRUCTIVE"),
        })
    }

    pub fn plan_options(&self) -> PlanOptions {
        PlanOptions { rejected_renames: self.rejected_renames.clone(), confirmed_renames: self.renames.clone() }
    }

    fn approves_plan_as_shown(&self) -> bool {
        self.legacy_approve || self.legacy_destructive
    }

    /// Takes a decision on every rename the planner inferred. Returns `true`
    /// when a decision was taken, so the plans have to be computed again: a
    /// rejected rename becomes a removal plus an addition, and a confirmed
    /// one loses its review warning.
    pub fn resolve_renames(&mut self, plans: &[&MigrationPlan], current: &DatabaseSchema) -> Result<bool, CliError> {
        let mut replan = false;
        let mut undecided = Vec::new();
        let inferred = plans.iter().flat_map(|plan| &plan.inferred_renames).collect::<BTreeSet<_>>();

        for rename in inferred {
            if self.renames.contains(rename) || self.rejected_renames.contains(rename) || self.approves_plan_as_shown()
            {
                continue;
            }
            if self.accept_renames {
                self.renames.insert(rename.clone());
                replan = true;
                continue;
            }
            if !prompt::is_interactive() {
                undecided.push(rename.clone());
                continue;
            }

            if ask_rename(rename, current)? {
                self.renames.insert(rename.clone());
            } else {
                self.rejected_renames.insert(rename.clone());
            }
            replan = true;
        }

        if !undecided.is_empty() {
            let mut error = CliError::decision_required("The migration plan contains renames that need confirmation.")
                .reason("Dinoco inferred these renames from matching structure. A wrong guess would attach existing data to the wrong field, so each one must be confirmed or rejected.");
            for rename in &undecided {
                error = error.detail("Rename", describe_rename(rename));
            }
            return Err(error
                .hint(format!("Confirm a rename with --rename {}", undecided[0].mapping()))
                .hint(format!("Plan it as a removal plus an addition with --no-rename {}", undecided[0].mapping()))
                .hint("Or confirm every inferred rename with --accept-renames"));
        }

        Ok(replan)
    }

    /// Fails when a `--rename` does not match any rename of the final plan,
    /// so a typo is never silently ignored.
    pub fn check_rename_options(&self, plans: &[&MigrationPlan]) -> Result<(), CliError> {
        let inferred = plans.iter().flat_map(|plan| &plan.inferred_renames).collect::<BTreeSet<_>>();
        if let Some(unknown) = self.renames.iter().find(|rename| !inferred.contains(rename)) {
            let mut error = CliError::usage("A --rename option does not match any rename in this migration plan.")
                .detail("Option", format!("--rename {}", unknown.mapping()));
            error = if inferred.is_empty() {
                error.reason("The planner did not infer any rename for the current schema changes.")
            } else {
                error.detail("Inferred", inferred.iter().map(|rename| rename.mapping()).collect::<Vec<_>>().join(", "))
            };
            return Err(error.hint("Check the table and field names, or remove the option."));
        }
        Ok(())
    }

    /// Chooses a replacement for every removed enum value that existing rows
    /// still hold. Rows are counted in the live database.
    pub async fn resolve_enum_replacements(
        &mut self,
        db: &CliDatabase,
        current: &DatabaseSchema,
        desired: &DatabaseSchema,
    ) -> anyhow::Result<Vec<EnumValueReplacement>> {
        let removals = enum_removals(db, current, desired).await?;
        let mut replacements = Vec::new();
        let mut undecided = Vec::new();

        for removal in removals {
            let key = (removal.enum_name.clone(), removal.value.clone());
            let replacement = match self.enum_replacements.get(&key) {
                Some(replacement) => {
                    if !removal.candidates.contains(replacement) {
                        return Err(CliError::usage("A --replace-enum-value option names an invalid replacement.")
                            .detail("Option", format!("--replace-enum-value {}.{}={replacement}", key.0, key.1))
                            .detail("Allowed", allowed_replacements(&removal.candidates))
                            .reason("A replacement must be a value that already exists before this migration and is kept by it.")
                            .into());
                    }
                    replacement.clone()
                }
                None if self.approves_plan_as_shown() => {
                    ui::warning(format!(
                        "{} still use enum value `{}` of `{}`, which this migration removes; they are left unchanged.",
                        ui::plural(removal.total_rows() as usize, "row", "rows"),
                        removal.value,
                        removal.enum_name
                    ));
                    continue;
                }
                None if prompt::is_interactive() => {
                    let replacement = ask_enum_replacement(&removal)?;
                    self.enum_replacements.insert(key, replacement.clone());
                    replacement
                }
                None => {
                    undecided.push(removal);
                    continue;
                }
            };

            replacements.extend(removal.usages.iter().map(|usage| EnumValueReplacement {
                table: usage.table.clone(),
                column: usage.column.clone(),
                enum_name: removal.enum_name.clone(),
                from: removal.value.clone(),
                to: replacement.clone(),
                rows: usage.rows,
            }));
        }

        if let Some(first) = undecided.first() {
            let mut error = CliError::decision_required("Removed enum values are still used by existing rows.")
                .reason("Each removed value needs a replacement, so the rows that use it stay valid.");
            for removal in &undecided {
                error = error.detail(
                    "Value",
                    format!(
                        "{}.{} ({}; replace with {})",
                        removal.enum_name,
                        removal.value,
                        ui::plural(removal.total_rows() as usize, "row", "rows"),
                        allowed_replacements(&removal.candidates)
                    ),
                );
            }
            let example = first.candidates.first().map(String::as_str).unwrap_or("VALUE");
            return Err(error
                .hint(format!(
                    "Choose a replacement with --replace-enum-value {}.{}={example}",
                    first.enum_name, first.value
                ))
                .into());
        }

        Ok(replacements)
    }

    /// Approves the plan from the options, or asks; `show_sql` prints the
    /// migration when the user asks to review it.
    pub fn approve_plan(&self, plans: &[&MigrationPlan], action: &str, show_sql: impl Fn()) -> Result<(), CliError> {
        let destructive = destructive_changes(plans);
        let approved = if destructive > 0 {
            self.allow_destructive || self.legacy_approve || self.legacy_destructive
        } else {
            self.yes || self.allow_destructive || self.legacy_approve
        };
        if approved {
            return Ok(());
        }

        if !prompt::is_interactive() {
            let error = if destructive > 0 {
                CliError::decision_required("This migration contains destructive changes and needs explicit approval.")
                    .detail("Destructive changes", destructive.to_string())
                    .hint("Review the Data Impact section above, then pass --allow-destructive to approve it.")
            } else {
                CliError::decision_required("This migration needs approval.")
                    .hint("Review the plan above, then pass --yes to approve it.")
            };
            return Err(error);
        }

        #[derive(Clone, Copy)]
        enum Action {
            Proceed,
            ShowSql,
            Cancel,
        }
        let question = if destructive > 0 {
            format!(
                "This plan has {} that may permanently delete data. Make sure you have a backup. How do you want to continue?",
                ui::plural(destructive, "destructive change", "destructive changes")
            )
        } else {
            "How do you want to continue?".to_string()
        };
        loop {
            let choices = vec![
                Choice::new(action, Action::Proceed),
                Choice::new("Show the migration SQL", Action::ShowSql),
                Choice::new("Cancel", Action::Cancel),
            ];
            // A destructive plan starts on "Cancel", so Enter alone never
            // approves data loss.
            match prompt::select(&question, choices, if destructive > 0 { 2 } else { 0 })? {
                Action::Proceed => return Ok(()),
                Action::ShowSql => show_sql(),
                Action::Cancel => {
                    return Err(CliError::cancelled("Migration generation cancelled. Nothing was written or applied."));
                }
            }
        }
    }

    /// Approves repairing schema drift.
    pub fn approve_drift_repair(&self) -> Result<(), CliError> {
        self.approve(
            self.repair_drift,
            "The database was changed outside Dinoco migrations. Record and apply a schema repair?",
            "Repairing schema drift needs explicit approval.",
            "Review the drift above, then pass --repair-drift to approve the repair.",
            "Schema repair cancelled. Nothing was written or applied.",
        )
    }

    /// Approves adopting existing, matching tables as the migration baseline.
    pub fn approve_baseline(&self) -> Result<(), CliError> {
        self.approve(
            self.adopt_baseline,
            "The existing tables match the Dinoco schema and will be adopted without being recreated. Record this baseline?",
            "Adopting the existing database as the migration baseline needs explicit approval.",
            "Review the tables above, then pass --adopt-baseline to record the baseline.",
            "Baseline adoption cancelled. Nothing was written or applied.",
        )
    }

    /// Approves recording a schema snapshot for migration history written by
    /// a release that did not record one.
    pub fn approve_history_adoption(&self) -> Result<(), CliError> {
        self.approve(
            self.adopt_baseline,
            "The applied migrations have no schema snapshot. Record the current database schema as their snapshot?",
            "Adopting the existing migration history needs explicit approval.",
            "Pass --adopt-baseline to record the snapshot.",
            "History adoption cancelled. Nothing was written.",
        )
    }

    /// Approves a destructive repair of SQLite drift.
    pub fn approve_destructive_repair(&self, plan: &MigrationPlan) -> Result<(), CliError> {
        let destructive = destructive_changes(&[plan]);
        self.approve(
            self.allow_destructive || self.legacy_destructive,
            &format!(
                "This repair has {} that may permanently delete data. Make sure you have a backup. Continue?",
                ui::plural(destructive, "destructive change", "destructive changes")
            ),
            "This repair contains destructive changes and needs explicit approval.",
            "Review the Data Impact section above, then pass --allow-destructive to approve it.",
            "Schema repair cancelled. Nothing was written or applied.",
        )
    }

    fn approve(
        &self,
        approved: bool,
        question: &str,
        required: &str,
        hint: &str,
        cancelled: &str,
    ) -> Result<(), CliError> {
        if approved {
            return Ok(());
        }
        if !prompt::is_interactive() {
            return Err(CliError::decision_required(required).hint(hint));
        }
        if prompt::confirm(question, false)? { Ok(()) } else { Err(CliError::cancelled(cancelled)) }
    }
}

fn parse_rename(mapping: &str, option: &str) -> Result<InferredRename, CliError> {
    let invalid = || {
        CliError::usage(format!("{option} expects TABLE.OLD=NEW for a column or OLD=NEW for a table."))
            .detail("Value", mapping)
            .hint(format!("Example: {option} user.username=display_name"))
    };
    let (from, to) = mapping.split_once('=').ok_or_else(invalid)?;
    let to = to.trim();
    if to.is_empty() || to.contains('.') {
        return Err(invalid());
    }
    match from.trim().split_once('.') {
        Some((table, column)) if !table.is_empty() && !column.is_empty() => {
            Ok(InferredRename::Column { table: table.to_string(), from: column.to_string(), to: to.to_string() })
        }
        Some(_) => Err(invalid()),
        None if !from.trim().is_empty() => {
            Ok(InferredRename::Table { from: from.trim().to_string(), to: to.to_string() })
        }
        None => Err(invalid()),
    }
}

fn parse_enum_replacement(mapping: &str) -> Result<((String, String), String), CliError> {
    let invalid = || {
        CliError::usage("--replace-enum-value expects ENUM.VALUE=REPLACEMENT.")
            .detail("Value", mapping)
            .hint("Example: --replace-enum-value Status.Processing=Pending")
    };
    let (removed, replacement) = mapping.split_once('=').ok_or_else(invalid)?;
    let (enum_name, value) = removed.trim().split_once('.').ok_or_else(invalid)?;
    let replacement = replacement.trim();
    if enum_name.is_empty() || value.is_empty() || replacement.is_empty() {
        return Err(invalid());
    }
    Ok(((enum_name.to_string(), value.to_string()), replacement.to_string()))
}

fn describe_rename(rename: &InferredRename) -> String {
    match rename {
        InferredRename::Table { from, to } => format!("table `{from}` -> `{to}` ({})", rename.mapping()),
        InferredRename::Column { table, from, to } => {
            format!("column `{table}.{from}` -> `{to}` ({})", rename.mapping())
        }
    }
}

fn ask_rename(rename: &InferredRename, current: &DatabaseSchema) -> Result<bool, CliError> {
    #[derive(Clone, Copy)]
    enum Answer {
        Rename,
        Replace,
        Cancel,
    }

    ui::heading("Migration Planning");
    let (question, yes, no) = match rename {
        InferredRename::Table { from, to } => {
            let rows = current.tables.iter().find(|table| table.name == *from).map_or(0, |table| table.row_count);
            ui::text("A possible table rename was detected.");
            ui::blank();
            ui::fields(&[("Removed", from.as_str()), ("Added", to.as_str()), ("Rows", &rows.to_string())]);
            (
                "Is this a table rename?",
                format!("Yes, rename `{from}` to `{to}` and keep its rows"),
                format!(
                    "No, drop `{from}` (its {} are deleted) and create `{to}`",
                    ui::plural(rows as usize, "row", "rows")
                ),
            )
        }
        InferredRename::Column { table, from, to } => {
            ui::text("A possible column rename was detected.");
            ui::blank();
            ui::fields(&[("Table", table.as_str()), ("Removed", from.as_str()), ("Added", to.as_str())]);
            (
                "Is this a column rename?",
                format!("Yes, rename `{from}` to `{to}` and keep its values"),
                format!("No, drop `{from}` (its values are deleted) and add `{to}`"),
            )
        }
    };
    ui::blank();

    let choices = vec![
        Choice::new(yes, Answer::Rename),
        Choice::new(no, Answer::Replace),
        Choice::new("Cancel migration planning", Answer::Cancel),
    ];
    match prompt::select(question, choices, 0)? {
        Answer::Rename => Ok(true),
        Answer::Replace => Ok(false),
        Answer::Cancel => Err(CliError::cancelled("Migration planning cancelled. Nothing was written or applied.")),
    }
}

/// A removed enum value that rows of the live database still hold.
struct EnumRemoval {
    enum_name: String,
    value: String,
    usages: Vec<EnumUsage>,
    /// Values that exist before the migration and are kept by it.
    candidates: Vec<String>,
}

struct EnumUsage {
    table: String,
    column: String,
    rows: i64,
}

impl EnumRemoval {
    fn total_rows(&self) -> i64 {
        self.usages.iter().map(|usage| usage.rows).sum()
    }
}

fn allowed_replacements(candidates: &[String]) -> String {
    if candidates.is_empty() { "none available".to_string() } else { candidates.join(", ") }
}

/// Enum values the migration removes from columns whose rows still hold
/// them. `desired` must carry the schema's enum names.
async fn enum_removals(
    db: &CliDatabase,
    current: &DatabaseSchema,
    desired: &DatabaseSchema,
) -> anyhow::Result<Vec<EnumRemoval>> {
    let mut removals = BTreeMap::<(String, String), EnumRemoval>::new();

    for desired_table in &desired.tables {
        let Some(current_table) = current.tables.iter().find(|table| table.name == desired_table.name) else {
            continue;
        };
        if current_table.row_count == 0 {
            continue;
        }

        for desired_column in &desired_table.columns {
            let MigrationColumnType::Enum { name: enum_name, values: desired_values } = &desired_column.ty else {
                continue;
            };
            let Some(MigrationColumnType::Enum { values: current_values, .. }) =
                current_table.columns.iter().find(|column| column.name == desired_column.name).map(|column| &column.ty)
            else {
                continue;
            };

            let candidates =
                desired_values.iter().filter(|value| current_values.contains(value)).cloned().collect::<Vec<_>>();
            for value in current_values.iter().filter(|value| !desired_values.contains(value)) {
                let rows = db.count_rows_with_value(&current_table.name, &desired_column.name, value).await?;
                if rows == 0 {
                    continue;
                }
                removals
                    .entry((enum_name.clone(), value.clone()))
                    .or_insert_with(|| EnumRemoval {
                        enum_name: enum_name.clone(),
                        value: value.clone(),
                        usages: Vec::new(),
                        candidates: candidates.clone(),
                    })
                    .usages
                    .push(EnumUsage { table: current_table.name.clone(), column: desired_column.name.clone(), rows });
            }
        }
    }

    Ok(removals.into_values().collect())
}

fn ask_enum_replacement(removal: &EnumRemoval) -> Result<String, CliError> {
    ui::heading("Enum Migration");
    ui::text(format!("The enum value \"{}\" is being removed, but existing rows still use it.", removal.value));
    ui::blank();
    let usage = removal
        .usages
        .iter()
        .map(|usage| format!("{} in {}.{}", ui::plural(usage.rows as usize, "row", "rows"), usage.table, usage.column))
        .collect::<Vec<_>>()
        .join(", ");
    ui::fields(&[("Enum", removal.enum_name.as_str()), ("Value", removal.value.as_str()), ("Usage", usage.as_str())]);
    ui::blank();

    if removal.candidates.is_empty() {
        return Err(CliError::failure("No value can replace a removed enum value that is still in use.")
            .detail("Enum", &removal.enum_name)
            .detail("Value", &removal.value)
            .reason("Every remaining value is new in this migration, and a value has to exist before rows can switch to it.")
            .hint(format!(
                "Keep \"{}\" in the schema, generate a migration that adds its replacement, then remove it in a following migration.",
                removal.value
            )));
    }

    let mut choices =
        removal.candidates.iter().map(|value| Choice::new(value.clone(), Some(value.clone()))).collect::<Vec<_>>();
    choices.push(Choice::new("Cancel migration planning", None));
    prompt::select("Select the replacement value:", choices, 0)?
        .ok_or_else(|| CliError::cancelled("Migration planning cancelled. Nothing was written or applied."))
}

fn destructive_changes(plans: &[&MigrationPlan]) -> usize {
    plans.iter().flat_map(|plan| &plan.warnings).filter(|warning| warning.destructive).count()
}

pub(crate) fn change_kind(step: &MigrationStep) -> Change {
    match step {
        MigrationStep::CreateEnum(_)
        | MigrationStep::CreateTable(_)
        | MigrationStep::AddColumn(_)
        | MigrationStep::AddForeignKey(_)
        | MigrationStep::CreateIndex(_) => Change::Add,
        MigrationStep::DropEnum(_)
        | MigrationStep::DropTable(_)
        | MigrationStep::DropColumn(_)
        | MigrationStep::DropForeignKey(_)
        | MigrationStep::DropIndex(_) => Change::Remove,
        MigrationStep::AlterEnum(_)
        | MigrationStep::RenameTable(_)
        | MigrationStep::AlterColumn(_)
        | MigrationStep::RenameColumn(_)
        | MigrationStep::RebuildTable(_)
        | MigrationStep::ReplaceEnumValue(_) => Change::Modify,
    }
}

/// Prints `plan` under `title`: every change, a one-line count, and the data
/// each change deletes or rewrites.
pub fn render_plan(title: &str, plan: &MigrationPlan, describe: impl Fn(&MigrationStep) -> String) {
    ui::heading(title);
    let mut counts = [0usize; 3];
    for step in &plan.steps {
        let kind = change_kind(step);
        counts[kind as usize] += 1;
        ui::change(kind, describe(step));
    }

    let parts = [
        (counts[Change::Add as usize], "addition", "additions"),
        (counts[Change::Modify as usize], "modification", "modifications"),
        (counts[Change::Remove as usize], "removal", "removals"),
    ]
    .into_iter()
    .filter(|(count, ..)| *count > 0)
    .map(|(count, singular, plural)| ui::plural(count, singular, plural))
    .collect::<Vec<_>>();
    ui::blank();
    ui::line(format!(
        "Detected {}: {}.",
        ui::plural(plan.steps.len(), "schema change", "schema changes"),
        parts.join(", ")
    ));

    if plan.warnings.is_empty() {
        return;
    }

    ui::heading("Data Impact");
    for warning in &plan.warnings {
        if warning.destructive {
            ui::impact(true, format!("Destructive change: {}", warning.message));
        } else {
            ui::impact(false, format!("Needs review: {}", warning.message));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_mappings_name_a_column_or_a_table() {
        assert_eq!(
            parse_rename("user.username=display_name", "--rename").unwrap(),
            InferredRename::Column {
                table: "user".to_string(),
                from: "username".to_string(),
                to: "display_name".to_string()
            }
        );
        assert_eq!(
            parse_rename("accounts=customers", "--rename").unwrap(),
            InferredRename::Table { from: "accounts".to_string(), to: "customers".to_string() }
        );
        for invalid in ["username", "user.username=", "=display_name", ".username=x", "user.=x", "a=b.c"] {
            let error = parse_rename(invalid, "--no-rename").unwrap_err();
            assert_eq!(error.exit_code(), 2, "{invalid}");
            assert!(error.to_string().starts_with("--no-rename expects TABLE.OLD=NEW"), "{error}");
        }
    }

    #[test]
    fn enum_replacements_name_the_enum_the_value_and_its_replacement() {
        assert_eq!(
            parse_enum_replacement("Status.Processing=Pending").unwrap(),
            (("Status".to_string(), "Processing".to_string()), "Pending".to_string())
        );
        for invalid in ["Status.Processing", "Processing=Pending", "Status.=Pending", "Status.Processing="] {
            assert!(parse_enum_replacement(invalid).is_err(), "{invalid}");
        }
    }
}
