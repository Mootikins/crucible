//! Workflow CLI commands: `list`, `show`, `start`, `approve`, `status`,
//! `cancel`.
//!
//! `list`/`show` are Phase 1+2: they read workflow notes from the active
//! kiln and render the parsed [`WorkflowDoc`] without involving the
//! daemon. `start`/`approve`/`status`/`cancel` are Phase 3a: they drive
//! the daemon-side workflow engine over RPC. See the plan at
//! `thoughts/shared/plans/workflows_2026-04-22-2030.md`.

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use crucible_core::parser::types::{
    CheckboxStatus, Frontmatter, FrontmatterFormat, ParsedNote, WorkflowDoc, WorkflowStep,
};
use crucible_core::EXCLUDED_DIRS;
use crucible_daemon::rpc_client::{WorkflowApproveGateRequest, WorkflowStartRequest};
use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::common::daemon_client;
use crate::config::CliConfig;
use crate::formatting::OutputFormat;

#[derive(Parser)]
pub struct WorkflowCommand {
    #[command(subcommand)]
    pub command: WorkflowSubcommand,
}

#[derive(Subcommand)]
pub enum WorkflowSubcommand {
    /// List all workflow notes in the active kiln
    List {
        /// Output format (table, json)
        #[arg(short = 'f', long, default_value = "table")]
        format: String,
    },
    /// Show a workflow's parsed structure (goals, validation, step tree)
    Show {
        /// Workflow identifier: a path, a title, or a filename stem.
        /// Relative paths are resolved against the active kiln.
        target: String,
        /// Output format (table, json)
        #[arg(short = 'f', long, default_value = "table")]
        format: String,
    },
    /// Start a workflow execution against a new session
    Start {
        /// Workflow identifier (path, filename stem, or title)
        target: String,
        /// Existing session id to attach to (otherwise a new workflow
        /// session is created against the active kiln).
        #[arg(long)]
        session: Option<String>,
    },
    /// Approve a pending gate on a running workflow
    Approve {
        /// Workflow session id
        session: String,
        /// Gate id (as emitted in a `workflow.gate_reached` event).
        /// Omit to approve whichever gate is currently pending.
        gate: Option<String>,
    },
    /// Show current status of a running workflow
    Status {
        /// Workflow session id
        session: String,
    },
    /// Cancel a running workflow
    Cancel {
        /// Workflow session id
        session: String,
    },
}

pub async fn execute(config: CliConfig, command: WorkflowSubcommand) -> Result<()> {
    match command {
        WorkflowSubcommand::List { format } => run_list(config, OutputFormat::from_str(&format)),
        WorkflowSubcommand::Show { target, format } => {
            run_show(config, &target, OutputFormat::from_str(&format))
        }
        WorkflowSubcommand::Start { target, session } => {
            run_start(config, &target, session.as_deref()).await
        }
        WorkflowSubcommand::Approve { session, gate } => {
            run_approve(&session, gate.as_deref()).await
        }
        WorkflowSubcommand::Status { session } => run_status(&session).await,
        WorkflowSubcommand::Cancel { session } => run_cancel(&session).await,
    }
}

// ---------- list ----------

#[derive(Debug, Serialize)]
struct ListEntry {
    path: String,
    title: String,
    description: Option<String>,
    steps_count: usize,
    goals_count: usize,
    validations_count: usize,
    gate_count: usize,
}

fn run_list(config: CliConfig, format: OutputFormat) -> Result<()> {
    let kiln_path = &config.kiln_path;
    if !kiln_path.exists() {
        return Err(anyhow!("Kiln path does not exist: {}", kiln_path.display()));
    }

    let docs = find_workflows(kiln_path)?;

    let entries: Vec<ListEntry> = docs
        .iter()
        .map(|wf| {
            let path = wf.path.strip_prefix(kiln_path).unwrap_or(&wf.path);
            let total_steps = wf.iter_steps().count();
            let gate_count =
                wf.preamble_gates.len() + wf.iter_steps().map(|s| s.gates.len()).sum::<usize>();
            ListEntry {
                path: path.display().to_string(),
                title: wf.title.clone(),
                description: wf.description.clone(),
                steps_count: total_steps,
                goals_count: wf.goals.len(),
                validations_count: wf.validations.len(),
                gate_count,
            }
        })
        .collect();

    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&entries)?);
        }
        _ => {
            if entries.is_empty() {
                println!(
                    "No workflows found in {} (no notes with `type: workflow` frontmatter).",
                    kiln_path.display()
                );
                return Ok(());
            }
            println!(
                "{:<40}  {:<30}  {:>5}  {:>5}  {:>5}  {:>5}",
                "PATH", "TITLE", "STEPS", "GOALS", "VAL", "GATES"
            );
            println!("{}", "-".repeat(100));
            for e in &entries {
                println!(
                    "{:<40}  {:<30}  {:>5}  {:>5}  {:>5}  {:>5}",
                    truncate(&e.path, 40),
                    truncate(&e.title, 30),
                    e.steps_count,
                    e.goals_count,
                    e.validations_count,
                    e.gate_count,
                );
            }
        }
    }

    Ok(())
}

// ---------- show ----------

fn run_show(config: CliConfig, target: &str, format: OutputFormat) -> Result<()> {
    let kiln_path = &config.kiln_path;
    let wf = resolve_workflow(kiln_path, target)?;

    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&wf)?);
        }
        _ => render_tree(&wf),
    }
    Ok(())
}

fn render_tree(wf: &WorkflowDoc) {
    println!("{}", wf.title);
    if let Some(desc) = &wf.description {
        println!("  {}", desc);
    }

    if !wf.goals.is_empty() {
        println!();
        println!("  Goals ({}):", wf.goals.len());
        for goal in &wf.goals {
            let sym = match goal.status {
                CheckboxStatus::Done => "x",
                _ => " ",
            };
            println!("    [{}] {}", sym, goal.content);
        }
    }

    if !wf.validations.is_empty() {
        println!();
        println!("  Validation ({}):", wf.validations.len());
        for v in &wf.validations {
            match &v.command {
                Some(cmd) => println!("    $ `{}` — {}", cmd, v.description),
                None => println!("    - {}", v.description),
            }
        }
    }

    for gate in &wf.preamble_gates {
        println!();
        match &gate.title {
            Some(t) => println!("  [GATE] {}", t),
            None => println!("  [GATE] {}", gate.content.lines().next().unwrap_or("")),
        }
    }

    if !wf.steps.is_empty() {
        println!();
        println!("  Steps:");
        for step in &wf.steps {
            render_step(step, 2);
        }
    }
}

fn render_step(step: &WorkflowStep, depth: usize) {
    let pad = "  ".repeat(depth);
    let mut line = step.title.clone();
    if step.parallel {
        // Echo the parser-stripped marker so the listing stays faithful
        // to the source, matching the @agent/-> output echoes below.
        line.insert(0, '&');
    }
    if let Some(agent) = &step.agent {
        line.push_str(&format!(" @{}", agent));
    }
    if let Some(output) = &step.output {
        line.push_str(&format!(" -> {}", output));
    }
    if let Some(type_attr) = step.attributes.get("type") {
        line.push_str(&format!(" [type:: {}]", type_attr));
    }
    println!("{}{}", pad, line);

    for gate in &step.gates {
        let label = gate
            .title
            .clone()
            .unwrap_or_else(|| gate.content.lines().next().unwrap_or("").to_string());
        println!("{}  [GATE] {}", pad, label);
    }

    for child in &step.children {
        render_step(child, depth + 1);
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(n.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

// ---------- kiln scanning + resolution ----------

fn find_workflows(kiln_path: &Path) -> Result<Vec<WorkflowDoc>> {
    let mut docs = Vec::new();
    walk_markdown(kiln_path, &mut |path| {
        if let Some(wf) = try_parse_workflow(path)? {
            docs.push(wf);
        }
        Ok(())
    })?;
    docs.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(docs)
}

fn resolve_workflow(kiln_path: &Path, target: &str) -> Result<WorkflowDoc> {
    // 1. Absolute or cwd-relative path.
    let direct = PathBuf::from(target);
    if direct.exists() {
        return try_parse_workflow(&direct)?
            .ok_or_else(|| anyhow!("{} is not a workflow note", direct.display()));
    }

    // 2. Path relative to kiln root.
    let kiln_rel = kiln_path.join(target);
    if kiln_rel.exists() {
        return try_parse_workflow(&kiln_rel)?
            .ok_or_else(|| anyhow!("{} is not a workflow note", kiln_rel.display()));
    }

    // 3. Scan kiln, match by filename stem or by title.
    let all = find_workflows(kiln_path)?;
    let matches: Vec<&WorkflowDoc> = all
        .iter()
        .filter(|wf| {
            let stem = wf.path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            stem.eq_ignore_ascii_case(target) || wf.title.eq_ignore_ascii_case(target)
        })
        .collect();

    match matches.len() {
        0 => Err(anyhow!(
            "No workflow found matching '{}' (searched as path, filename stem, and title)",
            target
        )),
        1 => Ok(matches[0].clone()),
        _ => {
            let mut msg = format!("Multiple workflows match '{}':\n", target);
            for m in &matches {
                let rel = m.path.strip_prefix(kiln_path).unwrap_or(&m.path);
                msg.push_str(&format!("  {} — {}\n", rel.display(), m.title));
            }
            msg.push_str("Pass a path to disambiguate.");
            Err(anyhow!(msg))
        }
    }
}

fn try_parse_workflow(path: &Path) -> Result<Option<WorkflowDoc>> {
    let source =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;

    // Fast-path: skip files that clearly aren't workflows without allocating a
    // full ParsedNote. The definitive check is the frontmatter below.
    if !source.contains("type") || !source.contains("workflow") {
        return Ok(None);
    }

    let fm = extract_yaml_frontmatter(&source);
    let mut note = ParsedNote::new(path.to_path_buf());
    note.frontmatter = fm;
    Ok(WorkflowDoc::from_parsed(&note, &source))
}

fn extract_yaml_frontmatter(source: &str) -> Option<Frontmatter> {
    let rest = source.strip_prefix("---\n")?;
    let end = rest.find("\n---\n")?;
    let yaml = &rest[..end];
    Some(Frontmatter::new(yaml.to_string(), FrontmatterFormat::Yaml))
}

fn walk_markdown<F>(root: &Path, visit: &mut F) -> Result<()>
where
    F: FnMut(&Path) -> Result<()>,
{
    if !root.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if EXCLUDED_DIRS.contains(&name) || name.starts_with('.') {
                continue;
            }
            walk_markdown(&path, visit)?;
        } else if path.is_file()
            && path
                .extension()
                .map(|e| e.eq_ignore_ascii_case("md"))
                .unwrap_or(false)
        {
            visit(&path)?;
        }
    }
    Ok(())
}

// ---------- execution commands (Phase 3a) ----------

async fn run_start(config: CliConfig, target: &str, _session: Option<&str>) -> Result<()> {
    // Locate the workflow note and keep the source for RPC transport.
    let (path, source) = load_workflow_source(&config.kiln_path, target)?;

    let client = daemon_client().await?;

    // Create a fresh workflow session. Reusing an existing one is a
    // follow-up — keeps this slice small and the invariant simple
    // (one active workflow per session).
    let create_resp = client
        .session_create(crucible_daemon::rpc_client::SessionCreateParams {
            session_type: "workflow".to_string(),
            kiln: Some(config.kiln_path.clone()),
            workspace: None,
            connect_kilns: Vec::new(),
            recording_mode: None,
            recording_path: None,
            agent_type: None,
        })
        .await
        .context("failed to create workflow session")?;
    let session_id = create_resp
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("daemon returned no session_id on session.create"))?
        .to_string();

    println!("Started workflow session: {}", session_id);
    println!("Workflow: {}", path.display());
    println!();

    let result = client
        .workflow_start(WorkflowStartRequest {
            session_id: session_id.clone(),
            source,
            path: Some(path.display().to_string()),
        })
        .await
        .context("workflow.start RPC failed")?;

    print_status_result(&result);
    Ok(())
}

async fn run_approve(session: &str, gate: Option<&str>) -> Result<()> {
    let client = daemon_client().await?;

    let gate_id = match gate {
        Some(g) => g.to_string(),
        None => gate_id_from_status(&client, session).await?,
    };

    let result = client
        .workflow_approve_gate(WorkflowApproveGateRequest {
            session_id: session.to_string(),
            gate_id,
        })
        .await
        .context("workflow.approve_gate RPC failed")?;

    print_status_result(&result);
    Ok(())
}

async fn run_status(session: &str) -> Result<()> {
    let client = daemon_client().await?;
    let snap = client
        .workflow_status(session)
        .await
        .context("workflow.status RPC failed")?;

    println!(
        "Progress: {} / {}",
        snap.get("completed_slots")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        snap.get("total_slots")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
    );
    if let Some(status) = snap.get("status") {
        println!("Status: {}", status);
    }
    if let Some(scope) = snap.get("scope") {
        if !scope.as_object().map(|o| o.is_empty()).unwrap_or(true) {
            println!("Outputs: {}", scope);
        }
    }
    Ok(())
}

async fn run_cancel(session: &str) -> Result<()> {
    let client = daemon_client().await?;
    client
        .workflow_cancel(session)
        .await
        .context("workflow.cancel RPC failed")?;
    println!("Cancelled workflow on session {}", session);
    Ok(())
}

fn load_workflow_source(kiln_path: &Path, target: &str) -> Result<(PathBuf, String)> {
    // Reuse the resolve logic, but return path + raw source so we can
    // send to the daemon unmodified.
    let direct = PathBuf::from(target);
    if direct.exists() {
        let src = std::fs::read_to_string(&direct)
            .with_context(|| format!("reading {}", direct.display()))?;
        if try_parse_workflow(&direct)?.is_none() {
            return Err(anyhow!("{} is not a workflow note", direct.display()));
        }
        return Ok((direct, src));
    }
    let kiln_rel = kiln_path.join(target);
    if kiln_rel.exists() {
        let src = std::fs::read_to_string(&kiln_rel)
            .with_context(|| format!("reading {}", kiln_rel.display()))?;
        if try_parse_workflow(&kiln_rel)?.is_none() {
            return Err(anyhow!("{} is not a workflow note", kiln_rel.display()));
        }
        return Ok((kiln_rel, src));
    }

    // Resolve via stem / title.
    let wf = resolve_workflow(kiln_path, target)?;
    let src = std::fs::read_to_string(&wf.path)
        .with_context(|| format!("reading {}", wf.path.display()))?;
    Ok((wf.path, src))
}

async fn gate_id_from_status(
    client: &crucible_daemon::DaemonClient,
    session: &str,
) -> Result<String> {
    let snap = client
        .workflow_status(session)
        .await
        .context("workflow.status RPC failed")?;
    snap.get("status")
        .and_then(|s| s.get("gate"))
        .and_then(|g| g.get("id"))
        .and_then(|id| id.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| {
            anyhow!(
                "no gate is currently pending on session '{}'; pass gate id explicitly",
                session
            )
        })
}

fn print_status_result(value: &serde_json::Value) {
    let Some(status) = value.get("status") else {
        println!(
            "{}",
            serde_json::to_string_pretty(value).unwrap_or_default()
        );
        return;
    };
    match status.get("kind").and_then(|v| v.as_str()) {
        Some("running") => println!("Status: running"),
        Some("awaiting_approval") => {
            let gate = status.get("gate");
            let gate_id = gate
                .and_then(|g| g.get("id"))
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let title = gate
                .and_then(|g| g.get("title"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            println!("Status: awaiting approval");
            if title.is_empty() {
                println!("  Gate: {}", gate_id);
            } else {
                println!("  Gate: {} — {}", gate_id, title);
            }
            println!();
            println!("Approve with: cru workflow approve <session> {}", gate_id);
        }
        Some("completed") => println!("Status: completed"),
        Some("failed") => {
            let reason = status.get("reason").and_then(|v| v.as_str()).unwrap_or("?");
            let at = status.get("at_step").and_then(|v| v.as_str()).unwrap_or("");
            if at.is_empty() {
                println!("Status: failed — {}", reason);
            } else {
                println!("Status: failed at step {} — {}", at, reason);
            }
        }
        Some("cancelled") => println!("Status: cancelled"),
        _ => println!(
            "{}",
            serde_json::to_string_pretty(status).unwrap_or_default()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write(path: &Path, body: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, body).unwrap();
    }

    fn kiln_config(root: &Path) -> CliConfig {
        CliConfig {
            kiln_path: root.to_path_buf(),
            ..Default::default()
        }
    }

    #[test]
    fn list_finds_workflows_and_skips_non_workflows() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        write(
            &root.join("a.md"),
            "---\ntype: workflow\ntitle: A\n---\n## Step 1\n",
        );
        write(
            &root.join("nested/b.md"),
            "---\ntype: workflow\ntitle: B\n---\n## Step\n### Child\n",
        );
        write(
            &root.join("not-a-workflow.md"),
            "---\ntype: note\n---\n# Hi\n",
        );
        write(&root.join("no-frontmatter.md"), "# Just a note\n");

        let docs = find_workflows(root).unwrap();
        assert_eq!(docs.len(), 2);
        let titles: Vec<_> = docs.iter().map(|d| d.title.as_str()).collect();
        assert!(titles.contains(&"A"));
        assert!(titles.contains(&"B"));
    }

    #[test]
    fn list_skips_excluded_and_hidden_dirs() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        write(
            &root.join(".crucible/workflow-in-config.md"),
            "---\ntype: workflow\n---\n## X\n",
        );
        write(
            &root.join("node_modules/workflow-in-deps.md"),
            "---\ntype: workflow\n---\n## X\n",
        );
        write(&root.join("visible.md"), "---\ntype: workflow\n---\n## X\n");

        let docs = find_workflows(root).unwrap();
        assert_eq!(docs.len(), 1);
        assert!(docs[0].path.ends_with("visible.md"));
    }

    #[test]
    fn resolve_by_path_filename_and_title() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        write(
            &root.join("deploy.md"),
            "---\ntype: workflow\ntitle: Deploy Feature\n---\n## Plan\n",
        );

        // By relative path.
        let wf = resolve_workflow(root, "deploy.md").unwrap();
        assert_eq!(wf.title, "Deploy Feature");

        // By filename stem.
        let wf = resolve_workflow(root, "deploy").unwrap();
        assert_eq!(wf.title, "Deploy Feature");

        // By title.
        let wf = resolve_workflow(root, "Deploy Feature").unwrap();
        assert_eq!(wf.title, "Deploy Feature");
    }

    #[test]
    fn resolve_unknown_target_errors() {
        let tmp = TempDir::new().unwrap();
        let err = resolve_workflow(tmp.path(), "nonexistent").unwrap_err();
        assert!(err.to_string().contains("nonexistent"));
    }

    #[test]
    fn resolve_ambiguous_title_errors() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write(
            &root.join("a/deploy.md"),
            "---\ntype: workflow\ntitle: Deploy\n---\n## X\n",
        );
        write(
            &root.join("b/deploy.md"),
            "---\ntype: workflow\ntitle: Deploy\n---\n## X\n",
        );
        let err = resolve_workflow(root, "Deploy").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Multiple workflows"));
        assert!(msg.contains("a/deploy.md") || msg.contains("a\\deploy.md"));
    }

    #[tokio::test]
    async fn execute_list_runs_against_empty_kiln() {
        let tmp = TempDir::new().unwrap();
        let config = kiln_config(tmp.path());
        let result = execute(
            config,
            WorkflowSubcommand::List {
                format: "table".into(),
            },
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn execute_show_errors_on_unknown() {
        let tmp = TempDir::new().unwrap();
        let config = kiln_config(tmp.path());
        let result = execute(
            config,
            WorkflowSubcommand::Show {
                target: "does-not-exist".into(),
                format: "table".into(),
            },
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn execute_show_json_emits_valid_json() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write(
            &root.join("wf.md"),
            "---\ntype: workflow\ntitle: X\n---\n## Step\n",
        );

        // The command prints to stdout; we only verify it succeeds here.
        let config = kiln_config(root);
        execute(
            config,
            WorkflowSubcommand::Show {
                target: "X".into(),
                format: "json".into(),
            },
        )
        .await
        .unwrap();
    }
}
