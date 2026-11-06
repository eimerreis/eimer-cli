//! `pipeline trigger [name...]`: queue a new run.

use anyhow::{Context, Result, anyhow, bail};
use clap::Args;
use serde_json::Value;

use super::common::{
    Run, az_json, get_repo_info, parse_key_value_pairs, resolve_string_arg, str_at,
};
use crate::core::process::run_json;
use crate::core::{prompt, ui};

#[derive(Args)]
pub struct TriggerArgs {
    /// Pipeline definition name (same as --name)
    #[arg(value_name = "NAME")]
    pipeline: Vec<String>,
    /// Pipeline definition ID
    #[arg(long, short = 'i', value_parser = clap::value_parser!(u64).range(1..))]
    id: Option<u64>,
    /// Pipeline definition name
    #[arg(long, short = 'm')]
    name: Option<String>,
    /// Branch name or full ref
    #[arg(long, short = 'b')]
    branch: Option<String>,
    /// Pipeline parameter key=value (repeatable)
    #[arg(long, short = 'p')]
    parameter: Vec<String>,
    /// Pipeline variable key=value (repeatable)
    #[arg(long, short = 'v')]
    variable: Vec<String>,
    /// Print machine-readable JSON
    #[arg(long, short = 'j')]
    json: bool,
}

pub fn run(args: TriggerArgs) -> Result<()> {
    trigger(args).context("Failed to trigger pipeline")
}

fn trigger(args: TriggerArgs) -> Result<()> {
    let name = resolve_string_arg(args.name.as_deref(), &args.pipeline);
    let definition = resolve_pipeline(args.id, &name, args.json)?;
    let definition_id = definition.get("id").and_then(Value::as_u64).unwrap_or(0);

    let id = definition_id.to_string();
    let mut cmd = az_json(&["pipelines", "run", "--id", &id]);
    if let Some(branch) = args
        .branch
        .as_deref()
        .map(str::trim)
        .filter(|b| !b.is_empty())
    {
        cmd.extend(["--branch".into(), branch.into()]);
    }
    let parameters = parse_key_value_pairs(&args.parameter);
    if !parameters.is_empty() {
        cmd.push("--parameters".into());
        cmd.extend(parameters);
    }
    let variables = parse_key_value_pairs(&args.variable);
    if !variables.is_empty() {
        cmd.push("--variables".into());
        cmd.extend(variables);
    }

    let value: Value = ui::spin_unless(args.json, "Queueing pipeline run", || run_json(&cmd))?;
    if args.json {
        return ui::print_json(&value);
    }

    let run = Run(&value);
    let name = run
        .str(&["definition", "name"])
        .or_else(|| str_at(&definition, &["name"]))
        .map(String::from)
        .unwrap_or_else(|| format!("Pipeline {definition_id}"));
    ui::print_success(&format!(
        "Queued pipeline run {} {} {name}",
        run.status_label(),
        run.link()
    ));
    Ok(())
}

fn show_definition(id: u64) -> Result<Value> {
    run_json(az_json(&["pipelines", "show", "--id", &id.to_string()]))
}

/// By `--id`, else exact (case-insensitive) name, else the repository's only pipeline
/// or an interactive pick among the repository's pipelines.
fn resolve_pipeline(id: Option<u64>, name: &str, json: bool) -> Result<Value> {
    if let Some(id) = id {
        return ui::spin_unless(json, "Resolving pipeline", || show_definition(id));
    }

    if !name.is_empty() {
        let definitions: Vec<Value> = ui::spin_unless(json, "Resolving pipeline", || {
            run_json(az_json(&[
                "pipelines",
                "list",
                "--name",
                name,
                "--top",
                "20",
            ]))
        })?;
        return definitions
            .into_iter()
            .find(|d| str_at(d, &["name"]).is_some_and(|n| n.eq_ignore_ascii_case(name)))
            .ok_or_else(|| anyhow!("No pipeline found with name '{name}'."));
    }

    let (repo, mut definitions) = ui::spin_unless(json, "Resolving pipeline", || -> Result<_> {
        let repo = get_repo_info()?;
        let definitions: Vec<Value> = run_json(az_json(&[
            "pipelines",
            "list",
            "--repository",
            &repo.repository_filter,
            "--repository-type",
            repo.repository_type,
            "--top",
            "100",
        ]))?;
        Ok((repo, definitions))
    })?;

    match definitions.len() {
        0 => bail!(
            "No pipelines found for repo '{}' ({}). Pass --id or --name to select pipeline directly.",
            repo.repository_filter,
            repo.repository_type
        ),
        1 => Ok(definitions.remove(0)),
        _ => {
            let items: Vec<String> = definitions.iter().map(choice_label).collect();
            let index = prompt::select("Select pipeline", &items, 0).map_err(|e| {
                anyhow!("Missing pipeline ID. Pass --id or use interactive selection. ({e})")
            })?;
            Ok(definitions.swap_remove(index))
        }
    }
}

fn choice_label(definition: &Value) -> String {
    format!(
        "#{} {}",
        definition.get("id").and_then(Value::as_u64).unwrap_or(0),
        str_at(definition, &["name"]).unwrap_or("Unnamed")
    )
}

#[cfg(test)]
mod tests {
    use super::choice_label;
    use serde_json::json;

    #[test]
    fn labels() {
        assert_eq!(choice_label(&json!({"id": 3, "name": "ci"})), "#3 ci");
        assert_eq!(choice_label(&json!({"id": 4})), "#4 Unnamed");
    }
}
