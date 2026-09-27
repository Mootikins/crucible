//! The canonical parser supplies the fence span; edits replace only its YAML bytes.
use super::*;
use std::ops::Range;
pub(super) async fn range(path: &Path, text: &str, yaml: &str) -> Result<Range<usize>> {
    let parsed = crucible_core::parser::CrucibleParser::new()
        .parse_content(text, path)
        .await?;
    let mut found = Vec::new();
    for block in parsed.content.blocks {
        if !matches!(block.kind, crucible_core::parser::types::BlockKind::Code { language: Some(ref l) } if l == "base")
        {
            continue;
        }
        let start = parsed.body_offset + block.start_offset;
        let end = parsed.body_offset + block.end_offset;
        let raw = &text[start..end];
        let Some(first_line) = raw.find('\n') else {
            continue;
        };
        let last = raw.trim_end_matches(['\r', '\n']);
        let Some(last_line) = last.rfind('\n') else {
            continue;
        };
        let content = &raw[first_line + 1..last_line + 1];
        if content.replace("\r\n", "\n").trim_end_matches('\n')
            == yaml.replace("\r\n", "\n").trim_end_matches('\n')
        {
            found.push(start + first_line + 1..start + last_line + 1);
        }
    }
    ensure!(
        found.len() == 1,
        "Inline base must match exactly one current host fence"
    );
    Ok(found.remove(0))
}
