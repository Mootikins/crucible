//! Workflow document types — a typed view over a [`ParsedNote`] when the
//! frontmatter declares `type: workflow`.
//!
//! Phase 1 shape: parse only. No execution. See
//! `thoughts/shared/plans/workflows_2026-04-22-2030.md` for the full design.
//!
//! # Ownership
//!
//! Canonical location for workflow AST. Re-exported from [`crate::parser`].

use super::{Callout, CheckboxStatus, Frontmatter, InlineMetadata, ParsedNote, TaskItem};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

// ---------- Public types ----------

/// Parsed workflow document.
///
/// Produced by [`WorkflowDoc::from_parsed`] when the note's frontmatter
/// declares `type: workflow`; otherwise returns `None`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDoc {
    /// Source note path.
    pub path: PathBuf,

    /// Workflow title (from frontmatter `title`, fallback: filename stem).
    pub title: String,

    /// Optional description from frontmatter.
    pub description: Option<String>,

    /// Full frontmatter (kept for tag/classification lookups).
    pub frontmatter: Frontmatter,

    /// Goals from the `## Goals` section. Empty if no section present.
    /// Reuses the existing [`TaskItem`] type — only `[ ]`/`[x]` are
    /// meaningful for workflow goals.
    pub goals: Vec<TaskItem>,

    /// Success/failure criteria from the `## Validation` section.
    /// Parallel to goals — goals describe *what* we're building, validations
    /// describe *how* we know it works. Empty if no section present.
    pub validations: Vec<ValidationEntry>,

    /// Top-level steps (tree; children nested via [`WorkflowStep::children`]).
    pub steps: Vec<WorkflowStep>,

    /// Gates that appear BEFORE any step heading (workflow-level gates,
    /// e.g. "this whole workflow requires prior approval").
    pub preamble_gates: Vec<Gate>,

    /// Non-fatal parser warnings.
    pub warnings: Vec<WorkflowParseWarning>,
}

/// A single step (one heading subtree).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    /// Heading level (1-6).
    pub level: u8,

    /// Step title, with `@agent`/`->`/`[k:: v]` suffix metadata stripped.
    /// May be empty if the heading contained only metadata.
    pub title: String,

    /// Optional agent hint extracted from a trailing `@<ident>` suffix.
    pub agent: Option<String>,

    /// Optional named output extracted from a trailing `-> <ident>` suffix.
    pub output: Option<String>,

    /// Inline metadata attributes from `[k:: v]` pairs in the heading.
    /// Canonical keys: `type`, `source`, `timeout`, `on_error`, but ANY key
    /// is accepted (runtime decides what it understands).
    pub attributes: HashMap<String, String>,

    /// Raw body text between this heading and its first child heading or
    /// its next sibling/parent heading. Preserves markdown so the Phase 3
    /// runtime can feed it as prompt material.
    pub body: String,

    /// Nested sub-steps (headings with level > `self.level`, up to the next
    /// sibling at level <= `self.level`).
    pub children: Vec<WorkflowStep>,

    /// Gates inside this step's direct body (not inside children).
    pub gates: Vec<Gate>,

    /// Byte offset in the full source (file-relative, including frontmatter).
    pub offset: usize,
}

/// A success/failure criterion from the `## Validation` section.
///
/// Phase 1 captures these as structured data. Phase 3+ will use them to
/// prime per-step agent context ("success looks like X, Y, Z") and to
/// assess workflow completion. Entries with a `command` are eventually
/// runnable; entries without one are manual/narrative checks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationEntry {
    /// Human-readable criterion text (with the extracted command stripped
    /// if any). Always non-empty.
    pub description: String,

    /// Optional runnable command. Populated when the source list item
    /// contained exactly one inline-code span — the span's content becomes
    /// the command. Zero or multiple spans → `None`.
    pub command: Option<String>,

    /// Byte offset in the full source (file-relative).
    pub offset: usize,
}

/// A gate callout `> [!gate]`, typed wrapper so downstream code doesn't
/// re-pattern-match on `CalloutType::Custom("gate")`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gate {
    /// Optional title from `> [!gate] Title`.
    pub title: Option<String>,
    /// Gate body text (continuation lines).
    pub content: String,
    /// Byte offset in the full source (file-relative).
    pub offset: usize,
}

impl From<&Callout> for Gate {
    fn from(c: &Callout) -> Self {
        Gate {
            title: c.title.clone(),
            content: c.content.clone(),
            offset: c.offset,
        }
    }
}

/// Non-fatal parser warnings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkflowParseWarning {
    DuplicateGoalsSection { offset: usize },
    DuplicateValidationSection { offset: usize },
}

// ---------- Impl ----------

impl WorkflowDoc {
    /// Try to interpret a [`ParsedNote`] as a workflow. Returns `None` if
    /// the frontmatter doesn't declare `type: workflow`.
    ///
    /// `source` is the full file content (frontmatter included); the parser
    /// strips frontmatter internally so offsets in the result are
    /// file-relative.
    pub fn from_parsed(note: &ParsedNote, source: &str) -> Option<Self> {
        let fm = note.frontmatter.as_ref()?;
        let type_val = fm.get_string("type")?;
        if !type_val.eq_ignore_ascii_case("workflow") {
            return None;
        }

        let body_start = body_start_offset(source);
        let body = &source[body_start..];

        let title = fm.get_string("title").unwrap_or_else(|| {
            note.path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Untitled")
                .to_string()
        });
        let description = fm.get_string("description");

        let all_headings = scan_headings(body);
        let gates_raw = scan_gates(body);
        let mut warnings = Vec::new();

        let (goals, goals_idx, mut goals_warnings) = extract_goals(body, &all_headings);
        warnings.append(&mut goals_warnings);

        let (validations, val_idx, mut val_warnings) =
            extract_validations(body, body_start, &all_headings);
        warnings.append(&mut val_warnings);

        let promoted: Vec<usize> = [goals_idx, val_idx].into_iter().flatten().collect();
        let step_headings = filter_out_sections(&all_headings, &promoted);

        let steps = build_tree(body, body_start, &step_headings, &gates_raw);

        let first_step_line = step_headings.first().map(|h| h.line).unwrap_or(usize::MAX);
        let preamble_gates: Vec<Gate> = gates_raw
            .iter()
            .filter(|g| g.line < first_step_line)
            .map(|g| Gate {
                title: g.title.clone(),
                content: g.content.clone(),
                offset: g.byte_offset + body_start,
            })
            .collect();

        Some(WorkflowDoc {
            path: note.path.clone(),
            title,
            description,
            frontmatter: fm.clone(),
            goals,
            validations,
            steps,
            preamble_gates,
            warnings,
        })
    }

    /// Flatten steps depth-first (parent first, then children).
    pub fn iter_steps(&self) -> StepIter<'_> {
        StepIter::new(&self.steps)
    }
}

/// Depth-first iterator over a step tree (parent, then children).
pub struct StepIter<'a> {
    stack: Vec<&'a WorkflowStep>,
}

impl<'a> StepIter<'a> {
    fn new(roots: &'a [WorkflowStep]) -> Self {
        let mut stack = Vec::with_capacity(roots.len());
        for s in roots.iter().rev() {
            stack.push(s);
        }
        StepIter { stack }
    }
}

impl<'a> Iterator for StepIter<'a> {
    type Item = &'a WorkflowStep;
    fn next(&mut self) -> Option<Self::Item> {
        let step = self.stack.pop()?;
        for child in step.children.iter().rev() {
            self.stack.push(child);
        }
        Some(step)
    }
}

// ---------- Internals ----------

/// A heading encountered while scanning the body. Keeps both line number
/// and byte offset (both body-relative) so we can do span math without
/// mixing units.
#[derive(Debug, Clone)]
struct RawHeading {
    level: u8,
    text: String,
    line: usize,
    byte_offset: usize,
}

/// A gate callout encountered while scanning the body.
#[derive(Debug, Clone)]
struct RawGate {
    title: Option<String>,
    content: String,
    line: usize,
    byte_offset: usize,
}

fn body_start_offset(source: &str) -> usize {
    if let Some(rest) = source.strip_prefix("---\n") {
        if let Some(end_idx) = rest.find("\n---\n") {
            return "---\n".len() + end_idx + "\n---\n".len();
        }
    }
    if let Some(rest) = source.strip_prefix("+++\n") {
        if let Some(end_idx) = rest.find("\n+++\n") {
            return "+++\n".len() + end_idx + "\n+++\n".len();
        }
    }
    0
}

fn heading_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(#{1,6})\s+(.+?)\s*$").unwrap())
}

fn gate_hdr_re() -> &'static Regex {
    static HDR: OnceLock<Regex> = OnceLock::new();
    HDR.get_or_init(|| Regex::new(r"^>\s*\[!gate\](?:\s+(.*?))?\s*$").unwrap())
}

/// A body line with pre-computed byte offset and "inside code fence" flag.
/// Both scanners share this so fence state is established once and applied
/// consistently.
#[derive(Debug, Clone, Copy)]
struct BodyLine<'a> {
    text: &'a str,
    byte_offset: usize,
    in_fence: bool,
}

fn body_lines(body: &str) -> Vec<BodyLine<'_>> {
    let mut out = Vec::new();
    let mut byte_pos = 0usize;
    let mut in_fence = false;
    let mut fence_marker: Option<String> = None;

    for line in body.split('\n') {
        let trimmed = line.trim_start();

        // Fence transitions happen on the fence line itself; that line is
        // "in fence" from the opener's perspective but the code inside is
        // what we want to ignore. Mark both the opening and closing fence
        // lines as in-fence so nothing inside a fence (including `> [!gate]`
        // in an example block) matches.
        let line_in_fence;
        if in_fence {
            line_in_fence = true;
            if let Some(marker) = &fence_marker {
                if trimmed.starts_with(marker.as_str()) {
                    in_fence = false;
                    fence_marker = None;
                }
            }
        } else if (trimmed.starts_with("```") || trimmed.starts_with("~~~"))
            && trimmed
                .chars()
                .take_while(|&c| c == '`' || c == '~')
                .count()
                >= 3
        {
            let ch = trimmed.chars().next().unwrap();
            let run_len = trimmed.chars().take_while(|&c| c == ch).count();
            in_fence = true;
            fence_marker = Some(std::iter::repeat_n(ch, run_len).collect::<String>());
            line_in_fence = true;
        } else {
            line_in_fence = false;
        }

        out.push(BodyLine {
            text: line,
            byte_offset: byte_pos,
            in_fence: line_in_fence,
        });

        byte_pos += line.len() + 1;
    }
    out
}

fn scan_headings(body: &str) -> Vec<RawHeading> {
    let mut headings = Vec::new();
    for (line_num, bl) in body_lines(body).into_iter().enumerate() {
        if bl.in_fence {
            continue;
        }
        if let Some(cap) = heading_re().captures(bl.text) {
            let hashes = cap.get(1).unwrap().as_str();
            let level = hashes.len() as u8;
            let text = cap
                .get(2)
                .unwrap()
                .as_str()
                .trim_end_matches('#')
                .trim_end()
                .to_string();
            headings.push(RawHeading {
                level,
                text,
                line: line_num,
                byte_offset: bl.byte_offset,
            });
        }
    }
    headings
}

fn scan_gates(body: &str) -> Vec<RawGate> {
    let hdr = gate_hdr_re();
    let lines = body_lines(body);
    let mut gates = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let bl = lines[i];
        if bl.in_fence || !hdr.is_match(bl.text) {
            i += 1;
            continue;
        }
        let cap = hdr.captures(bl.text).unwrap();
        let title = cap
            .get(1)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        let start_line = i;
        let start_byte = bl.byte_offset;
        i += 1;

        let mut content_lines = Vec::new();
        while i < lines.len() {
            let cont = lines[i];
            if cont.in_fence {
                break;
            }
            // Continuation: starts with `>`. Content is everything after the
            // leading `>` (and one optional space). A new gate header ends
            // the current gate.
            if hdr.is_match(cont.text) {
                break;
            }
            if let Some(rest) = cont.text.strip_prefix('>') {
                let rest = rest.strip_prefix(' ').unwrap_or(rest);
                content_lines.push(rest.to_string());
                i += 1;
            } else {
                break;
            }
        }

        gates.push(RawGate {
            title,
            content: content_lines.join("\n"),
            line: start_line,
            byte_offset: start_byte,
        });
    }
    gates
}

/// Regex for Dataview-style inline metadata with a stricter-than-global
/// key pattern: the key may not cross `]` (so a heading like
/// `## Impl [TICKET-123] @dev [priority:: high]` parses correctly — the
/// global `extract_inline_metadata` uses `[^:]+` which greedily consumes
/// the first `]` and the surrounding prose).
fn workflow_meta_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\s*\[([^:\]]+?)::\s*([^\]]+)\]").unwrap())
}

/// Parse a heading's text into (title, agent, output, attributes).
///
/// Operates strictly on the *trailing* tokens: `[k:: v]` pairs come off
/// first (anywhere in the text, matching Dataview convention), then a
/// trailing `-> <ident>`, then a trailing `@<ident>`.
fn parse_heading_suffix(
    text: &str,
) -> (
    String,
    Option<String>,
    Option<String>,
    HashMap<String, String>,
) {
    // 1. Extract `[k:: v]` pairs and strip them (anywhere in the text).
    let meta_re = workflow_meta_re();
    let mut attributes: HashMap<String, String> = HashMap::new();
    for cap in meta_re.captures_iter(text) {
        let key = cap.get(1).unwrap().as_str().trim().to_string();
        let value = cap.get(2).unwrap().as_str().trim().to_string();
        attributes.insert(key, value);
    }

    let stripped = meta_re.replace_all(text, "").trim().to_string();

    // 2. Trailing `-> <ident>` (only ASCII bare idents, no spaces).
    static OUT_RE: OnceLock<Regex> = OnceLock::new();
    let out_re =
        OUT_RE.get_or_init(|| Regex::new(r"\s+->\s+([a-zA-Z_][a-zA-Z0-9_-]*)\s*$").unwrap());
    let (output, after_output) = if let Some(cap) = out_re.captures(&stripped) {
        let out = cap.get(1).unwrap().as_str().to_string();
        let full = cap.get(0).unwrap();
        (Some(out), stripped[..full.start()].trim_end().to_string())
    } else {
        (None, stripped.clone())
    };

    // 3. Trailing `@<ident>` (allows hyphens in agent names).
    static AGENT_RE: OnceLock<Regex> = OnceLock::new();
    let agent_re =
        AGENT_RE.get_or_init(|| Regex::new(r"(?:^|\s)@([a-zA-Z_][a-zA-Z0-9_-]*)\s*$").unwrap());
    let (agent, after_agent) = if let Some(cap) = agent_re.captures(&after_output) {
        let ag = cap.get(1).unwrap().as_str().to_string();
        let full = cap.get(0).unwrap();
        (
            Some(ag),
            after_output[..full.start()].trim_end().to_string(),
        )
    } else {
        (None, after_output)
    };

    (after_agent.trim().to_string(), agent, output, attributes)
}

fn extract_goals(
    body: &str,
    headings: &[RawHeading],
) -> (Vec<TaskItem>, Option<usize>, Vec<WorkflowParseWarning>) {
    let indices: Vec<usize> = headings
        .iter()
        .enumerate()
        .filter(|(_, h)| h.level == 2 && h.text.trim() == "Goals")
        .map(|(i, _)| i)
        .collect();

    if indices.is_empty() {
        return (Vec::new(), None, Vec::new());
    }

    let mut warnings = Vec::new();
    for extra in indices.iter().take(indices.len() - 1) {
        warnings.push(WorkflowParseWarning::DuplicateGoalsSection {
            offset: headings[*extra].byte_offset,
        });
    }
    let idx = *indices.last().unwrap();
    let section = section_body_slice(body, headings, idx);
    let items = parse_goal_items(section);
    (items, Some(idx), warnings)
}

fn extract_validations(
    body: &str,
    body_start: usize,
    headings: &[RawHeading],
) -> (
    Vec<ValidationEntry>,
    Option<usize>,
    Vec<WorkflowParseWarning>,
) {
    let indices: Vec<usize> = headings
        .iter()
        .enumerate()
        .filter(|(_, h)| h.level == 2 && h.text.trim() == "Validation")
        .map(|(i, _)| i)
        .collect();

    if indices.is_empty() {
        return (Vec::new(), None, Vec::new());
    }

    let mut warnings = Vec::new();
    for extra in indices.iter().take(indices.len() - 1) {
        warnings.push(WorkflowParseWarning::DuplicateValidationSection {
            offset: headings[*extra].byte_offset,
        });
    }
    let idx = *indices.last().unwrap();
    let section_start = end_of_line(body, headings[idx].byte_offset);
    let section = section_body_slice(body, headings, idx);

    let entries = parse_validation_items(section, section_start + body_start);
    (entries, Some(idx), warnings)
}

fn section_body_slice<'a>(body: &'a str, headings: &[RawHeading], idx: usize) -> &'a str {
    let start = end_of_line(body, headings[idx].byte_offset);
    let section_level = headings[idx].level;
    let end = headings
        .iter()
        .skip(idx + 1)
        .find(|h| h.level <= section_level)
        .map(|h| h.byte_offset)
        .unwrap_or(body.len());
    &body[start..end]
}

fn end_of_line(body: &str, byte_offset: usize) -> usize {
    let rest = &body[byte_offset..];
    match rest.find('\n') {
        Some(nl) => byte_offset + nl + 1,
        None => body.len(),
    }
}

fn parse_goal_items(section: &str) -> Vec<TaskItem> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"^[ \t]*-\s*\[(.)\]\s*(.+)$").unwrap());

    let meta_re = workflow_meta_re();
    let mut items = Vec::new();
    for line in section.lines() {
        if let Some(cap) = re.captures(line) {
            let status_char = cap[1].chars().next().unwrap_or(' ');
            let raw_content = cap[2].to_string();
            let status = CheckboxStatus::from_char(status_char).unwrap_or(CheckboxStatus::Pending);

            // Use the stricter `workflow_meta_re` (key pattern doesn't cross
            // `]`) so goal text like `[TICKET-123] do the thing [id:: g1]`
            // extracts {id: "g1"}, not {TICKET-123] do the thing [id: "g1"}.
            let mut metadata_map: HashMap<String, InlineMetadata> = HashMap::new();
            for m in meta_re.captures_iter(&raw_content) {
                let key = m.get(1).unwrap().as_str().trim().to_string();
                let value_str = m.get(2).unwrap().as_str().trim();
                let values: Vec<String> =
                    value_str.split(',').map(|v| v.trim().to_string()).collect();
                metadata_map.insert(key.clone(), InlineMetadata { key, values });
            }
            let clean = meta_re.replace_all(&raw_content, "").trim().to_string();

            items.push(TaskItem::new(clean, status, metadata_map));
        }
    }
    items
}

fn parse_validation_items(section: &str, section_start_in_source: usize) -> Vec<ValidationEntry> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re =
        RE.get_or_init(|| Regex::new(r"^[ \t]*-\s*(?:\[(?:.)\])?\s*(?P<content>.+)$").unwrap());

    let mut entries = Vec::new();
    let mut cursor = 0usize; // byte offset inside `section`
    for line in section.split('\n') {
        if let Some(cap) = re.captures(line) {
            if let Some(content) = cap.name("content") {
                let entry =
                    parse_validation_line(content.as_str(), section_start_in_source + cursor);
                entries.push(entry);
            }
        }
        cursor += line.len() + 1;
    }
    entries
}

fn parse_validation_line(content: &str, offset: usize) -> ValidationEntry {
    static CODE_RE: OnceLock<Regex> = OnceLock::new();
    let code_re = CODE_RE.get_or_init(|| Regex::new(r"`([^`]+)`").unwrap());

    let spans: Vec<String> = code_re
        .captures_iter(content)
        .map(|c| c.get(1).unwrap().as_str().to_string())
        .collect();

    if spans.len() == 1 {
        let command = spans[0].clone();
        let description = code_re.replace(content, "").trim().to_string();
        if description.is_empty() {
            return ValidationEntry {
                description: command,
                command: None,
                offset,
            };
        }
        return ValidationEntry {
            description,
            command: Some(command),
            offset,
        };
    }

    ValidationEntry {
        description: content.trim().to_string(),
        command: None,
        offset,
    }
}

/// Remove the `## Goals` / `## Validation` heading subtrees from the list
/// used for step tree construction. We skip the heading AND any headings
/// nested under it (up to the next heading at the same or shallower level).
fn filter_out_sections<'a>(headings: &'a [RawHeading], promoted: &[usize]) -> Vec<&'a RawHeading> {
    // Build skip mask.
    let mut skip = vec![false; headings.len()];
    for &root in promoted {
        let level = headings[root].level;
        skip[root] = true;
        for j in (root + 1)..headings.len() {
            if headings[j].level <= level {
                break;
            }
            skip[j] = true;
        }
    }
    headings
        .iter()
        .enumerate()
        .filter(|(i, _)| !skip[*i])
        .map(|(_, h)| h)
        .collect()
}

fn build_tree(
    body: &str,
    body_start: usize,
    headings: &[&RawHeading],
    gates: &[RawGate],
) -> Vec<WorkflowStep> {
    build_subtree(body, body_start, headings, 0, 0, gates).0
}

fn build_subtree(
    body: &str,
    body_start: usize,
    headings: &[&RawHeading],
    start: usize,
    parent_level: u8,
    gates: &[RawGate],
) -> (Vec<WorkflowStep>, usize) {
    let mut steps = Vec::new();
    let mut i = start;
    while i < headings.len() {
        let h = headings[i];
        if h.level <= parent_level {
            return (steps, i);
        }
        let step_level = h.level;

        let this_body_start = end_of_line(body, h.byte_offset);
        let next_sibling_idx = ((i + 1)..headings.len()).find(|&j| headings[j].level <= step_level);
        let this_body_end = next_sibling_idx
            .map(|j| headings[j].byte_offset)
            .unwrap_or(body.len());
        let end_line = next_sibling_idx
            .map(|j| headings[j].line)
            .unwrap_or(usize::MAX);

        let (children, next_i) =
            build_subtree(body, body_start, headings, i + 1, step_level, gates);

        let first_child_line = ((i + 1)..next_i.min(headings.len()))
            .next()
            .map(|j| headings[j].line)
            .unwrap_or(end_line);
        let first_child_byte = ((i + 1)..next_i.min(headings.len()))
            .next()
            .map(|j| headings[j].byte_offset)
            .unwrap_or(this_body_end);

        let body_text = body[this_body_start..first_child_byte].trim().to_string();

        let (title, agent, output, attributes) = parse_heading_suffix(&h.text);

        let step_gates: Vec<Gate> = gates
            .iter()
            .filter(|g| g.line > h.line && g.line < first_child_line)
            .map(|g| Gate {
                title: g.title.clone(),
                content: g.content.clone(),
                offset: g.byte_offset + body_start,
            })
            .collect();

        steps.push(WorkflowStep {
            level: step_level,
            title,
            agent,
            output,
            attributes,
            body: body_text,
            children,
            gates: step_gates,
            offset: h.byte_offset + body_start,
        });
        i = next_i;
    }
    (steps, i)
}

// ---------- Tests ----------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::types::{Frontmatter, FrontmatterFormat, ParsedNote};
    use std::path::PathBuf;

    fn parse_workflow(source: &str) -> Option<WorkflowDoc> {
        parse_workflow_at(source, "test.md")
    }

    fn parse_workflow_at(source: &str, path: &str) -> Option<WorkflowDoc> {
        let (fm, _) = split_frontmatter(source);
        let mut note = ParsedNote::new(PathBuf::from(path));
        note.frontmatter = fm;
        WorkflowDoc::from_parsed(&note, source)
    }

    fn split_frontmatter(source: &str) -> (Option<Frontmatter>, String) {
        if let Some(rest) = source.strip_prefix("---\n") {
            if let Some(end) = rest.find("\n---\n") {
                let yaml = &rest[..end];
                let body = rest[end + "\n---\n".len()..].to_string();
                return (
                    Some(Frontmatter::new(yaml.to_string(), FrontmatterFormat::Yaml)),
                    body,
                );
            }
        }
        (None, source.to_string())
    }

    // ---- from_parsed / frontmatter gate ----

    #[test]
    fn non_workflow_note_returns_none() {
        let source = "---\ntype: note\n---\n# Hi\n";
        assert!(parse_workflow(source).is_none());
    }

    #[test]
    fn note_without_frontmatter_returns_none() {
        let source = "# Hi\n";
        assert!(parse_workflow(source).is_none());
    }

    #[test]
    fn note_without_type_field_returns_none() {
        let source = "---\ntitle: X\n---\n## Step\n";
        assert!(parse_workflow(source).is_none());
    }

    #[test]
    fn workflow_type_is_case_insensitive() {
        let source = "---\ntype: Workflow\n---\n## Step\n";
        assert!(parse_workflow(source).is_some());
    }

    #[test]
    fn empty_workflow_returns_some_with_no_steps() {
        let source = "---\ntype: workflow\n---\n";
        let wf = parse_workflow(source).expect("parses");
        assert!(wf.steps.is_empty());
        assert!(wf.goals.is_empty());
        assert!(wf.validations.is_empty());
    }

    #[test]
    fn title_from_frontmatter() {
        let source = "---\ntype: workflow\ntitle: Deploy Feature\n---\n";
        let wf = parse_workflow(source).expect("parses");
        assert_eq!(wf.title, "Deploy Feature");
    }

    #[test]
    fn title_falls_back_to_filename() {
        let source = "---\ntype: workflow\n---\n";
        let wf = parse_workflow_at(source, "deploy-feature.md").expect("parses");
        assert_eq!(wf.title, "deploy-feature");
    }

    // ---- heading suffix extraction (table-driven) ----

    #[test]
    fn heading_suffix_plain_title() {
        let (title, agent, output, attrs) = parse_heading_suffix("Simple Step");
        assert_eq!(title, "Simple Step");
        assert_eq!(agent, None);
        assert_eq!(output, None);
        assert!(attrs.is_empty());
    }

    #[test]
    fn heading_suffix_agent_only() {
        let (title, agent, _, _) = parse_heading_suffix("Design @architect");
        assert_eq!(title, "Design");
        assert_eq!(agent, Some("architect".to_string()));
    }

    #[test]
    fn heading_suffix_output_only() {
        let (title, _, output, _) = parse_heading_suffix("Parse -> config");
        assert_eq!(title, "Parse");
        assert_eq!(output, Some("config".to_string()));
    }

    #[test]
    fn heading_suffix_full_combo() {
        let (title, agent, output, attrs) = parse_heading_suffix("Step @a -> b [type:: fan]");
        assert_eq!(title, "Step");
        assert_eq!(agent, Some("a".to_string()));
        assert_eq!(output, Some("b".to_string()));
        assert_eq!(attrs.get("type").map(String::as_str), Some("fan"));
    }

    #[test]
    fn heading_suffix_version_arrow_not_matched() {
        // 2.0 is not a bare ident, so -> must not be extracted.
        let (title, _, output, _) = parse_heading_suffix("Migration: 1.0 -> 2.0");
        assert_eq!(title, "Migration: 1.0 -> 2.0");
        assert_eq!(output, None);
    }

    #[test]
    fn heading_suffix_middle_at_not_matched() {
        // @example.com is in the middle, not trailing; must remain in title.
        let (title, agent, _, _) = parse_heading_suffix("Parse RFC 5322 @example.com addresses");
        assert_eq!(title, "Parse RFC 5322 @example.com addresses");
        assert_eq!(agent, None);
    }

    #[test]
    fn heading_suffix_trailing_backticks_fine() {
        let (title, agent, _, _) = parse_heading_suffix("Implement `foo()` @dev");
        assert_eq!(title, "Implement `foo()`");
        assert_eq!(agent, Some("dev".to_string()));
    }

    #[test]
    fn heading_suffix_metadata_only() {
        let (title, _, _, attrs) = parse_heading_suffix("[type:: fan]");
        assert_eq!(title, "");
        assert_eq!(attrs.get("type").map(String::as_str), Some("fan"));
    }

    #[test]
    fn heading_suffix_multiple_attrs() {
        let (_, _, _, attrs) = parse_heading_suffix("Step [priority:: high] [owner:: alice]");
        assert_eq!(attrs.get("priority").map(String::as_str), Some("high"));
        assert_eq!(attrs.get("owner").map(String::as_str), Some("alice"));
    }

    #[test]
    fn heading_suffix_only_agent() {
        let (title, agent, _, _) = parse_heading_suffix("@only-agent");
        assert_eq!(title, "");
        assert_eq!(agent, Some("only-agent".to_string()));
    }

    // ---- tree structure ----

    #[test]
    fn tree_flat_siblings() {
        let source = "---\ntype: workflow\n---\n## A\n## B\n## C\n";
        let wf = parse_workflow(source).unwrap();
        assert_eq!(wf.steps.len(), 3);
        assert!(wf.steps.iter().all(|s| s.children.is_empty()));
    }

    #[test]
    fn tree_parent_with_children() {
        let source = "\
---
type: workflow
---
## Deploy
### Staging
### Production
";
        let wf = parse_workflow(source).unwrap();
        assert_eq!(wf.steps.len(), 1);
        assert_eq!(wf.steps[0].title, "Deploy");
        assert_eq!(wf.steps[0].children.len(), 2);
        assert_eq!(wf.steps[0].children[0].title, "Staging");
        assert_eq!(wf.steps[0].children[1].title, "Production");
    }

    #[test]
    fn tree_level_skip_still_nests() {
        // Level-2 then level-4 — level-4 still a child (we don't enforce
        // strict increments).
        let source = "---\ntype: workflow\n---\n## A\n#### Deep\n## B\n";
        let wf = parse_workflow(source).unwrap();
        assert_eq!(wf.steps.len(), 2);
        assert_eq!(wf.steps[0].children.len(), 1);
        assert_eq!(wf.steps[0].children[0].title, "Deep");
    }

    #[test]
    fn level_one_heading_treated_as_step() {
        // We don't try to be clever about "document title" — if they use `#`
        // as a step, it's a step. The common pattern is `## Step` anyway.
        let source = "---\ntype: workflow\n---\n# Whole Workflow\n## Sub\n";
        let wf = parse_workflow(source).unwrap();
        assert_eq!(wf.steps.len(), 1);
        assert_eq!(wf.steps[0].title, "Whole Workflow");
        assert_eq!(wf.steps[0].children.len(), 1);
    }

    #[test]
    fn headings_inside_code_fence_ignored() {
        let source = "\
---
type: workflow
---
## Real Step

```markdown
## Fake Step
```

## Another Real
";
        let wf = parse_workflow(source).unwrap();
        let titles: Vec<_> = wf.steps.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, vec!["Real Step", "Another Real"]);
    }

    // ---- goals ----

    #[test]
    fn goals_extracted_as_task_items() {
        let source = "\
---
type: workflow
---
## Goals

- [ ] Ship CSV export
- [x] Ship JSON export
- [ ] Cancelable for large files

## Implement
";
        let wf = parse_workflow(source).unwrap();
        assert_eq!(wf.goals.len(), 3);
        assert_eq!(wf.goals[0].content, "Ship CSV export");
        assert_eq!(wf.goals[0].status, CheckboxStatus::Pending);
        assert_eq!(wf.goals[1].status, CheckboxStatus::Done);
        // Goals heading is NOT in the steps tree.
        assert_eq!(wf.steps.len(), 1);
        assert_eq!(wf.steps[0].title, "Implement");
    }

    #[test]
    fn goals_with_inline_metadata() {
        let source = "\
---
type: workflow
---
## Goals

- [ ] First goal [id:: g1]
- [ ] Second goal [id:: g2] [priority:: high]
";
        let wf = parse_workflow(source).unwrap();
        assert_eq!(wf.goals[0].id, "g1");
        assert_eq!(wf.goals[1].id, "g2");
        assert_eq!(
            wf.goals[1]
                .metadata
                .get("priority")
                .and_then(|m| m.as_string()),
            Some("high")
        );
    }

    #[test]
    fn goals_ignores_non_task_bullets() {
        let source = "\
---
type: workflow
---
## Goals

- [ ] Real goal
- Just a bullet, not a goal
- [x] Another goal
";
        let wf = parse_workflow(source).unwrap();
        assert_eq!(wf.goals.len(), 2);
    }

    #[test]
    fn nested_goals_heading_not_promoted() {
        let source = "\
---
type: workflow
---
## Implement

### Goals

- [ ] Not a top-level goal
";
        let wf = parse_workflow(source).unwrap();
        assert!(wf.goals.is_empty());
        // Nested `### Goals` stays in the tree as a child step.
        assert_eq!(wf.steps.len(), 1);
        assert_eq!(wf.steps[0].children.len(), 1);
    }

    #[test]
    fn duplicate_goals_last_wins_with_warning() {
        let source = "\
---
type: workflow
---
## Goals

- [ ] First-section goal

## Goals

- [ ] Second-section goal
";
        let wf = parse_workflow(source).unwrap();
        assert_eq!(wf.goals.len(), 1);
        assert_eq!(wf.goals[0].content, "Second-section goal");
        assert_eq!(wf.warnings.len(), 1);
        assert!(matches!(
            wf.warnings[0],
            WorkflowParseWarning::DuplicateGoalsSection { .. }
        ));
    }

    #[test]
    fn task_list_not_under_goals_stays_in_step() {
        let source = "\
---
type: workflow
---
## Implement

- [ ] This is a step-level task, NOT a goal
";
        let wf = parse_workflow(source).unwrap();
        assert!(wf.goals.is_empty());
    }

    // ---- validation ----

    #[test]
    fn validation_entries_extract_commands() {
        let source = "\
---
type: workflow
---
## Validation

- `cargo test` passes
- `cargo clippy --all-targets` clean
- Manual: CSV download under 2s for 10k rows
";
        let wf = parse_workflow(source).unwrap();
        assert_eq!(wf.validations.len(), 3);
        assert_eq!(wf.validations[0].command.as_deref(), Some("cargo test"));
        assert_eq!(wf.validations[0].description, "passes");
        assert_eq!(
            wf.validations[1].command.as_deref(),
            Some("cargo clippy --all-targets")
        );
        assert_eq!(wf.validations[1].description, "clean");
        assert_eq!(wf.validations[2].command, None);
        assert_eq!(
            wf.validations[2].description,
            "Manual: CSV download under 2s for 10k rows"
        );
    }

    #[test]
    fn validation_multiple_code_spans_treated_as_prose() {
        let source = "\
---
type: workflow
---
## Validation

- Run `cargo test` then `cargo clippy`
";
        let wf = parse_workflow(source).unwrap();
        assert_eq!(wf.validations.len(), 1);
        assert_eq!(wf.validations[0].command, None);
    }

    #[test]
    fn validation_bare_command_falls_back_to_description() {
        let source = "\
---
type: workflow
---
## Validation

- `cargo fmt --check`
";
        let wf = parse_workflow(source).unwrap();
        assert_eq!(wf.validations.len(), 1);
        // Fallback rule: description holds the command text, no command field.
        assert_eq!(wf.validations[0].description, "cargo fmt --check");
        assert_eq!(wf.validations[0].command, None);
    }

    #[test]
    fn validation_task_style_items_still_parse() {
        let source = "\
---
type: workflow
---
## Validation

- [ ] `cargo test` passes
- [x] Performance acceptable
";
        let wf = parse_workflow(source).unwrap();
        assert_eq!(wf.validations.len(), 2);
        assert_eq!(wf.validations[0].command.as_deref(), Some("cargo test"));
        assert_eq!(wf.validations[1].description, "Performance acceptable");
    }

    #[test]
    fn validation_missing_section_is_empty() {
        let source = "---\ntype: workflow\n---\n## Step\n";
        let wf = parse_workflow(source).unwrap();
        assert!(wf.validations.is_empty());
    }

    #[test]
    fn validation_without_goals_still_parses() {
        let source = "\
---
type: workflow
---
## Validation

- `cargo test` passes
";
        let wf = parse_workflow(source).unwrap();
        assert!(wf.goals.is_empty());
        assert_eq!(wf.validations.len(), 1);
    }

    #[test]
    fn duplicate_validation_last_wins_with_warning() {
        let source = "\
---
type: workflow
---
## Validation

- First section

## Validation

- Second section
";
        let wf = parse_workflow(source).unwrap();
        assert_eq!(wf.validations.len(), 1);
        assert_eq!(wf.validations[0].description, "Second section");
        assert!(wf
            .warnings
            .iter()
            .any(|w| matches!(w, WorkflowParseWarning::DuplicateValidationSection { .. })));
    }

    #[test]
    fn nested_validation_heading_not_promoted() {
        let source = "\
---
type: workflow
---
## Implement

### Validation

- `cargo test` passes
";
        let wf = parse_workflow(source).unwrap();
        assert!(wf.validations.is_empty());
    }

    // ---- gates ----

    #[test]
    fn gate_inside_step_body() {
        let source = "\
---
type: workflow
---
## Deploy

> [!gate]
> Requires ops sign-off
";
        let wf = parse_workflow(source).unwrap();
        assert_eq!(wf.steps.len(), 1);
        assert_eq!(wf.steps[0].gates.len(), 1);
        assert!(wf.steps[0].gates[0].content.contains("ops sign-off"));
        assert!(wf.preamble_gates.is_empty());
    }

    #[test]
    fn gate_with_title() {
        let source = "\
---
type: workflow
---
## Deploy

> [!gate] Ops approval
> Body
";
        let wf = parse_workflow(source).unwrap();
        assert_eq!(wf.steps[0].gates[0].title.as_deref(), Some("Ops approval"));
    }

    #[test]
    fn gate_before_first_step_is_preamble() {
        let source = "\
---
type: workflow
---
> [!gate]
> Whole-workflow prior approval

## Do Thing
";
        let wf = parse_workflow(source).unwrap();
        assert_eq!(wf.preamble_gates.len(), 1);
        assert_eq!(wf.steps[0].gates.len(), 0);
    }

    #[test]
    fn gate_belongs_to_parent_not_child() {
        let source = "\
---
type: workflow
---
## Deploy

> [!gate]
> Approval required

### Staging
";
        let wf = parse_workflow(source).unwrap();
        assert_eq!(wf.steps[0].gates.len(), 1);
        assert_eq!(wf.steps[0].children[0].gates.len(), 0);
    }

    #[test]
    fn non_gate_callouts_ignored() {
        let source = "\
---
type: workflow
---
## Step

> [!note]
> Just a note
";
        let wf = parse_workflow(source).unwrap();
        assert_eq!(wf.steps[0].gates.len(), 0);
    }

    // ---- step bodies and output references ----

    #[test]
    fn step_body_captures_markdown_between_headings() {
        let source = "\
---
type: workflow
---
## Analyze

First, read the config.
Then, summarize findings.

## Implement
";
        let wf = parse_workflow(source).unwrap();
        assert_eq!(wf.steps[0].title, "Analyze");
        assert!(wf.steps[0].body.contains("read the config"));
        assert!(wf.steps[0].body.contains("summarize findings"));
        assert!(!wf.steps[0].body.contains("## Implement"));
    }

    #[test]
    fn output_suffix_populates_step_output() {
        let source = "---\ntype: workflow\n---\n## Parse -> config\n";
        let wf = parse_workflow(source).unwrap();
        assert_eq!(wf.steps[0].output.as_deref(), Some("config"));
        assert_eq!(wf.steps[0].title, "Parse");
    }

    // ---- fixture-style full examples ----

    #[test]
    fn deploy_example_from_docs_parses() {
        let source = "\
---
type: workflow
title: Deploy New Feature
---

## Goals

- [ ] Users can export data in CSV format
- [ ] Export respects active filters
- [ ] Large exports don't block the UI

## Plan the Implementation

Analyze requirements and identify affected components.

## Implement Changes @developer

Make code changes following existing patterns.

## Review and Deploy

> [!gate]
> Requires sign-off before production deployment

### Code Review @reviewer

### Deploy to Staging

### Deploy to Production
";
        let wf = parse_workflow(source).unwrap();
        assert_eq!(wf.title, "Deploy New Feature");
        assert_eq!(wf.goals.len(), 3);
        assert_eq!(wf.steps.len(), 3);
        assert_eq!(wf.steps[1].agent.as_deref(), Some("developer"));
        let deploy = &wf.steps[2];
        assert_eq!(deploy.title, "Review and Deploy");
        assert_eq!(deploy.gates.len(), 1);
        assert_eq!(deploy.children.len(), 3);
        assert_eq!(deploy.children[0].agent.as_deref(), Some("reviewer"));
    }

    #[test]
    fn data_flow_example_parses() {
        let source = "\
---
type: workflow
title: Data Flow
---

## Parse Configuration -> config

Read config.

## Validate Schema -> validated_config

Validate **config**.

## Generate Output

Use **validated_config**.
";
        let wf = parse_workflow(source).unwrap();
        assert_eq!(wf.steps.len(), 3);
        assert_eq!(wf.steps[0].output.as_deref(), Some("config"));
        assert_eq!(wf.steps[1].output.as_deref(), Some("validated_config"));
        assert_eq!(wf.steps[2].output, None);
    }

    #[test]
    fn iter_steps_depth_first() {
        let source = "\
---
type: workflow
---
## A
### A1
### A2
## B
### B1
";
        let wf = parse_workflow(source).unwrap();
        let titles: Vec<_> = wf.iter_steps().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, vec!["A", "A1", "A2", "B", "B1"]);
    }

    // ---- regression tests from code review ----

    #[test]
    fn gate_inside_code_fence_is_not_attached() {
        // Docs often contain fenced examples that *show* gate syntax. Those
        // must not create real gates on the enclosing step.
        let source = "\
---
type: workflow
---
## Step With Example

Here's how to write a gate:

```markdown
> [!gate]
> This is an example in docs
```

Real step content.
";
        let wf = parse_workflow(source).unwrap();
        assert_eq!(wf.steps.len(), 1);
        assert_eq!(wf.steps[0].gates.len(), 0);
        assert!(wf.preamble_gates.is_empty());
    }

    #[test]
    fn heading_with_bracketed_ticket_and_metadata_parses_correctly() {
        // The global `extract_inline_metadata` regex uses `[^:]+` for the
        // key, which greedily consumes the first `]` in a heading like
        // `## Impl [TICKET-123] @dev [priority:: high]` and produces a
        // nonsense key. The workflow parser uses a stricter regex that
        // doesn't cross `]` boundaries.
        let (title, agent, _output, attrs) =
            parse_heading_suffix("Implement [TICKET-123] @dev [priority:: high]");
        assert_eq!(
            attrs.get("priority").map(String::as_str),
            Some("high"),
            "priority attribute should be extracted cleanly"
        );
        assert_eq!(attrs.len(), 1, "no spurious attributes: {:?}", attrs);
        assert_eq!(agent.as_deref(), Some("dev"));
        assert_eq!(title, "Implement [TICKET-123]");
    }

    #[test]
    fn goals_with_bracketed_ticket_and_metadata_parse_correctly() {
        let source = "\
---
type: workflow
---
## Goals

- [ ] Fix [TICKET-123] auth bug [id:: g1]
- [ ] Ship [TICKET-456] feature [id:: g2] [priority:: high]
";
        let wf = parse_workflow(source).unwrap();
        assert_eq!(wf.goals.len(), 2);
        assert_eq!(wf.goals[0].id, "g1");
        assert!(
            wf.goals[0].content.contains("TICKET-123"),
            "content preserves ticket reference: {:?}",
            wf.goals[0].content
        );
        assert_eq!(wf.goals[1].id, "g2");
        assert_eq!(
            wf.goals[1]
                .metadata
                .get("priority")
                .and_then(|m| m.as_string()),
            Some("high")
        );
    }

    #[test]
    fn heading_inside_tilde_fence_ignored() {
        // Previously we only handled backtick fences; tilde fences are also
        // legal CommonMark and should mask their contents.
        let source = "\
---
type: workflow
---
## Real Step

~~~md
## Fake Step Inside Tilde Fence
~~~

## Another Real
";
        let wf = parse_workflow(source).unwrap();
        let titles: Vec<_> = wf.steps.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, vec!["Real Step", "Another Real"]);
    }
}
