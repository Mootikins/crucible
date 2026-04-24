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

#[cfg(test)]
mod tests;
