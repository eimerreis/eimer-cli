//! Shared task helpers: context init, work item model, az queries, rendering.

use anyhow::Result;
use serde::Serialize;
use serde_json::{Map, Value};

use crate::core::azure::{AzScope, same_project};
use crate::core::config::{EimerConfig, load_config};
use crate::core::context::resolve_current;
use crate::core::process::run_json;
use crate::core::ui::{self, symbols};

pub const DEFAULT_TEAM: &str = "Default Team";
pub const DEFAULT_AREA_PATH: &str = "Company\\Engineering";

/// State names meaning "not started yet" across process templates
/// (Agile: New, Scrum: To Do, Basic/custom: Not Started, CMMI: Proposed).
pub const NOT_STARTED_STATES: &[&str] = &["New", "To Do", "Not Started", "Proposed"];

/// Per-invocation task environment: config + resolved scope.
pub struct TaskEnv {
    pub config: EimerConfig,
    pub scope: AzScope,
}

impl TaskEnv {
    /// Resolves the context (flag > env > .eimerrc > defaultContext) and warns when it
    /// targets a different project than the cwd's git remote (unless `quiet`).
    pub fn init(flag: Option<&str>, quiet: bool) -> Result<Self> {
        let config = load_config()?;
        let context = resolve_current(flag, &config)?;

        if let (Some(ctx), false) = (&context, quiet)
            && let Some(remote) = crate::core::azure::remote_project()
        {
            let scope_project = AzScope::with_context(Some(ctx.clone())).project();
            if scope_project.is_some_and(|p| !same_project(&p, &remote)) {
                ui::print_warning(
                    &format!(
                        "Using context '{}' ({}/{}) from {}, but this repo points to {}/{}.",
                        ctx.name,
                        ctx.org,
                        ctx.project,
                        ctx.describe_source(),
                        remote.org,
                        remote.project
                    ),
                    None,
                );
            }
        }

        Ok(Self {
            config,
            scope: AzScope::with_context(context),
        })
    }

    pub fn context_name(&self) -> Option<&str> {
        self.scope.context.as_ref().map(|c| c.name.as_str())
    }

    pub fn default_team(&self) -> String {
        self.scope
            .context
            .as_ref()
            .and_then(|c| c.team.clone())
            .or_else(|| {
                self.config
                    .task
                    .as_ref()
                    .and_then(|t| t.default_team.clone())
            })
            .unwrap_or_else(|| DEFAULT_TEAM.to_string())
    }

    pub fn default_area_path(&self) -> String {
        self.scope
            .context
            .as_ref()
            .and_then(|c| c.area_path.clone())
            .or_else(|| {
                self.config
                    .task
                    .as_ref()
                    .and_then(|t| t.default_area_path.clone())
            })
            .unwrap_or_else(|| DEFAULT_AREA_PATH.to_string())
    }

    /// Current iteration path of `team`, or `None` if it has no current sprint.
    pub fn current_iteration(&self, team: &str) -> Option<String> {
        let mut cmd: Vec<String> = [
            "az",
            "boards",
            "iteration",
            "team",
            "list",
            "--timeframe",
            "current",
        ]
        .map(String::from)
        .to_vec();
        cmd.extend(self.scope.args());
        cmd.extend([
            "--output".into(),
            "json".into(),
            "--team".into(),
            team.into(),
        ]);

        let iterations: Vec<Value> = run_json(&cmd).ok()?;
        let current = iterations
            .iter()
            .find(|it| {
                it["attributes"]["timeFrame"]
                    .as_str()
                    .is_some_and(|t| t.eq_ignore_ascii_case("current"))
            })
            .or_else(|| iterations.first())?;
        current["path"].as_str().map(String::from)
    }

    /// Runs a WIQL query and returns the raw work items.
    pub fn query(&self, wiql: &str) -> Result<Vec<WorkItem>> {
        let mut cmd: Vec<String> = vec![
            "az".into(),
            "boards".into(),
            "query".into(),
            "--wiql".into(),
            wiql.into(),
        ];
        cmd.extend(self.scope.args());
        cmd.extend(["--output".into(), "json".into()]);
        run_json(&cmd)
    }

    pub fn show_work_item(&self, id: u64, relations: bool) -> Result<WorkItem> {
        let mut cmd: Vec<String> = ["az", "boards", "work-item", "show", "--id"]
            .map(String::from)
            .to_vec();
        cmd.push(id.to_string());
        cmd.extend(self.scope.org_args());
        cmd.extend(["--output".into(), "json".into()]);
        if relations {
            cmd.extend(["--expand".into(), "relations".into()]);
        }
        run_json(&cmd)
    }

    pub fn work_item_url(&self, id: u64) -> String {
        self.scope.work_item_url(id)
    }
}

#[derive(Debug, Clone, serde::Deserialize, Serialize)]
pub struct WorkItem {
    pub id: u64,
    #[serde(default)]
    pub fields: Map<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relations: Option<Vec<Relation>>,
}

#[derive(Debug, Clone, serde::Deserialize, Serialize)]
pub struct Relation {
    #[serde(default)]
    pub rel: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
}

impl WorkItem {
    pub fn field_str(&self, key: &str) -> Option<&str> {
        self.fields.get(key).and_then(Value::as_str)
    }

    pub fn title(&self) -> String {
        self.field_str("System.Title")
            .unwrap_or("Untitled")
            .to_string()
    }

    pub fn state(&self) -> String {
        self.field_str("System.State")
            .unwrap_or("Unknown")
            .to_string()
    }

    pub fn item_type(&self) -> String {
        self.field_str("System.WorkItemType")
            .unwrap_or("Work Item")
            .to_string()
    }

    pub fn assigned_to(&self) -> String {
        match self.fields.get("System.AssignedTo") {
            Some(Value::String(s)) if !s.is_empty() => s.clone(),
            Some(Value::Object(o)) => o
                .get("displayName")
                .or_else(|| o.get("uniqueName"))
                .and_then(Value::as_str)
                .unwrap_or("Unassigned")
                .to_string(),
            _ => "Unassigned".into(),
        }
    }

    pub fn description(&self) -> Option<String> {
        self.field_str("Custom.Description")
            .or_else(|| self.field_str("System.Description"))
            .map(String::from)
    }

    fn related_ids(&self, kind: &str) -> Vec<u64> {
        let mut ids = Vec::new();
        for relation in self.relations.iter().flatten() {
            let (Some(rel), Some(url)) = (&relation.rel, &relation.url) else {
                continue;
            };
            if !rel.to_lowercase().contains(kind) {
                continue;
            }
            if let Some(id) = url.rsplit('/').next().and_then(|s| s.parse().ok())
                && !ids.contains(&id)
            {
                ids.push(id);
            }
        }
        ids
    }

    pub fn child_ids(&self) -> Vec<u64> {
        self.related_ids("hierarchy-forward")
    }

    pub fn parent_id(&self) -> Option<u64> {
        self.related_ids("hierarchy-reverse").into_iter().next()
    }

    pub fn to_list_item(&self) -> ListItem {
        ListItem {
            id: self
                .fields
                .get("System.Id")
                .and_then(Value::as_u64)
                .unwrap_or(self.id),
            title: self.title(),
            state: self.state(),
            item_type: self.item_type(),
            assigned_to: self.assigned_to(),
            changed_date: self.field_str("System.ChangedDate").map(String::from),
        }
    }
}

/// Normalized row used in list output and `--json`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListItem {
    pub id: u64,
    pub title: String,
    pub state: String,
    #[serde(rename = "type")]
    pub item_type: String,
    pub assigned_to: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changed_date: Option<String>,
}

pub fn state_label(state: &str) -> String {
    let state = if state.trim().is_empty() {
        "Unknown"
    } else {
        state.trim()
    };
    match state.to_lowercase().as_str() {
        "active" => format!("{} {}", symbols::running(), ui::info(state)),
        "resolved" => format!("{} {}", symbols::pending(), ui::warning(state)),
        "not started" | "new" | "to do" | "proposed" => {
            format!("{} {}", symbols::queued(), ui::dim(state))
        }
        "closed" | "done" => format!("{} {}", symbols::ok(), ui::success(state)),
        "removed" => format!("{} {}", symbols::fail(), ui::error(state)),
        _ => format!("{} {}", symbols::neutral(), ui::dim(state)),
    }
}

pub fn print_items(env: &TaskEnv, items: &[ListItem]) {
    let rows = items.iter().map(|item| {
        vec![
            state_label(&item.state),
            ui::terminal_link(&format!("#{}", item.id), &env.work_item_url(item.id)),
            item.title.clone(),
            item.assigned_to.clone(),
            match ui::relative_time(item.changed_date.as_deref()) {
                s if s.is_empty() => "-".to_string(),
                s => s,
            },
        ]
    });
    println!(
        "{}",
        ui::render_table(["State", "Task", "Title", "Assigned", "Updated"], rows)
    );
}

pub const LIST_FIELDS: &str = "[System.Id], [System.Title], [System.State], [System.AssignedTo], [System.ChangedDate], [System.WorkItemType]";

/// `ID` resolution shared by start/close/show: positional or prompt.
pub fn resolve_id(id: Option<u64>, usage: &str) -> Result<u64> {
    match id {
        Some(id) => Ok(id),
        None => crate::core::prompt::id("Task ID")
            .map_err(|e| anyhow::anyhow!("Missing task ID. Usage: {usage} ({e})")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(json: &str) -> WorkItem {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn parses_relations_and_fields() {
        let wi = item(
            r#"{"id":5,"fields":{"System.Title":"Story","System.AssignedTo":{"displayName":"Me"}},
                "relations":[{"rel":"System.LinkTypes.Hierarchy-Forward","url":"https://x/_apis/wit/workItems/7"},
                             {"rel":"System.LinkTypes.Hierarchy-Reverse","url":"https://x/_apis/wit/workItems/2"}]}"#,
        );
        assert_eq!(wi.child_ids(), vec![7]);
        assert_eq!(wi.parent_id(), Some(2));
        assert_eq!(wi.assigned_to(), "Me");
        assert_eq!(wi.to_list_item().state, "Unknown");
    }
}
