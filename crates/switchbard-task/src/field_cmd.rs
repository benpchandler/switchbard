//! The `field` subcommand family — declaring, editing, listing, and removing
//! a repo's custom task fields (`backlog/config.yml`'s `fields:` list), plus
//! the shared `--set`/`--unset`/`--where` parsing `main.rs` and `list()` use.
//!
//! Output contract, same discipline as every other verb family: `add`
//! prints the new field's name alone; `edit`/`remove` print `Edited <NAME>`
//! / `Removed <NAME>`; `list` prints one TSV row per field (name, kind,
//! groupable, values).

use anyhow::{anyhow, ensure, Result};
use clap::{Args, Subcommand};
use std::path::Path;
use switchbard_core::{FieldDecl, FieldEditPatch, FieldKind};

#[derive(Subcommand)]
pub enum FieldCmd {
    /// Declare a new custom field; prints the name alone
    Add(FieldAddArgs),
    /// Edit an existing field's `values` and/or `groupable`; prints
    /// `Edited <NAME>`
    Edit(FieldEditArgs),
    /// Remove a field's declaration; refuses while any active or completed
    /// task still sets it (run `sb edit <ID> --unset <NAME>` on each first)
    Remove { name: String },
    /// List declared fields, one tab-separated row: name, kind, groupable,
    /// values (comma-joined, enum only)
    List,
}

#[derive(Args)]
pub struct FieldAddArgs {
    /// Field name: lowercase letters, digits, `_` only, starting with a letter
    pub name: String,
    /// text, enum, date, or person
    #[arg(long)]
    pub kind: String,
    /// Enum values, comma-separated and in declared (sort/section) order —
    /// required for `--kind enum`, refused for every other kind
    #[arg(long, value_delimiter = ',')]
    pub values: Vec<String>,
    /// Offer this field as a grouping/section axis
    #[arg(long)]
    pub groupable: bool,
}

#[derive(Args)]
pub struct FieldEditArgs {
    pub name: String,
    /// Replace the declared values wholesale (enum fields only)
    #[arg(long, value_delimiter = ',', conflicts_with = "no_groupable")]
    pub values: Option<Vec<String>>,
    /// Mark this field groupable
    #[arg(long, conflicts_with = "no_groupable")]
    pub groupable: bool,
    /// Mark this field not groupable
    #[arg(long)]
    pub no_groupable: bool,
}

pub fn run_field(root: &Path, cmd: &FieldCmd) -> Result<()> {
    match cmd {
        FieldCmd::Add(args) => add(root, args),
        FieldCmd::Edit(args) => edit(root, args),
        FieldCmd::Remove { name } => remove(root, name),
        FieldCmd::List => list(root),
    }
}

fn add(root: &Path, args: &FieldAddArgs) -> Result<()> {
    let kind = FieldKind::parse(&args.kind)?;
    let decl = FieldDecl {
        name: args.name.clone(),
        kind,
        values: args.values.clone(),
        groupable: args.groupable,
    };
    switchbard_core::add_field_decl(root, decl)?;
    println!("{}", args.name);
    Ok(())
}

fn edit(root: &Path, args: &FieldEditArgs) -> Result<()> {
    let groupable = if args.groupable {
        Some(true)
    } else if args.no_groupable {
        Some(false)
    } else {
        None
    };
    let patch = FieldEditPatch {
        values: args.values.clone(),
        groupable,
    };
    switchbard_core::edit_field_decl(root, &args.name, &patch)?;
    println!("Edited {}", args.name);
    Ok(())
}

fn remove(root: &Path, name: &str) -> Result<()> {
    let repo = switchbard_core::load_backlog_repo(root)?;
    let holders = switchbard_core::tasks_setting_field(&repo, name);
    ensure!(
        holders.is_empty(),
        "field `{name}` is still set on {}: {} — unset it first with `sb edit <ID> --unset {name}`",
        if holders.len() == 1 {
            "1 task"
        } else {
            "these tasks"
        },
        holders.join(", ")
    );
    switchbard_core::remove_field_decl(root, name)?;
    println!("Removed {name}");
    Ok(())
}

fn list(root: &Path) -> Result<()> {
    for field in switchbard_core::declared_fields(root)? {
        println!(
            "{}\t{}\t{}\t{}",
            field.name,
            field.kind.as_str(),
            field.groupable,
            field.values.join(",")
        );
    }
    Ok(())
}

/// One `--set NAME=VALUE` or `--where NAME=VALUE` token split at its first
/// `=`. Shared by `create`/`edit`'s `--set` and `list`'s `--where`.
pub fn parse_pair(raw: &str) -> Result<(String, String)> {
    let (name, value) = raw
        .split_once('=')
        .ok_or_else(|| anyhow!("`{raw}` is not NAME=VALUE (missing `=`)"))?;
    ensure!(!name.is_empty(), "`{raw}` has an empty field name");
    Ok((name.to_string(), value.to_string()))
}

pub fn parse_pairs(raw: &[String]) -> Result<Vec<(String, String)>> {
    raw.iter().map(|s| parse_pair(s)).collect()
}

/// Validate `--set NAME=VALUE` pairs at the `sb` boundary (Rule 5): every
/// name must be declared and every value must pass its declaration's
/// [`switchbard_core::validate_field_value`]. Called once before building
/// the patch/`NewBacklogTask`; the write layer trusts the result.
pub fn validate_set_pairs(root: &Path, pairs: &[(String, String)]) -> Result<()> {
    if pairs.is_empty() {
        return Ok(());
    }
    let fields = switchbard_core::declared_fields(root)?;
    for (name, value) in pairs {
        let decl = find_decl(&fields, name)?;
        switchbard_core::validate_field_value(decl, value)?;
    }
    Ok(())
}

/// Validate `--unset NAME` tokens: the name must be a declared field.
pub fn validate_unset_names(root: &Path, names: &[String]) -> Result<()> {
    if names.is_empty() {
        return Ok(());
    }
    let fields = switchbard_core::declared_fields(root)?;
    for name in names {
        find_decl(&fields, name)?;
    }
    Ok(())
}

fn find_decl<'f>(fields: &'f [FieldDecl], name: &str) -> Result<&'f FieldDecl> {
    fields.iter().find(|f| f.name == name).ok_or_else(|| {
        let known: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
        if known.is_empty() {
            anyhow!(
                "unknown field `{name}` — this repo declares no custom fields (see `sb field add`)"
            )
        } else {
            anyhow!("unknown field `{name}` (declared: {})", known.join(", "))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pair_splits_at_the_first_equals() {
        assert_eq!(
            parse_pair("counterparty=Nick").unwrap(),
            ("counterparty".to_string(), "Nick".to_string())
        );
        assert_eq!(
            parse_pair("note=a=b").unwrap(),
            ("note".to_string(), "a=b".to_string())
        );
        assert!(parse_pair("no-equals-sign").is_err());
        assert!(parse_pair("=value").is_err());
    }

    #[test]
    fn field_definition_is_internally_consistent() {
        use clap::CommandFactory;
        #[derive(clap::Parser)]
        struct Wrapper {
            #[command(subcommand)]
            cmd: FieldCmd,
        }
        Wrapper::command().debug_assert();
    }
}
