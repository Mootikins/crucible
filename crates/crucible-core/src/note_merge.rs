//! A line-level three-way merge for a note two writers changed.
//!
//! A stale write has three texts: the base the writer started from, the text
//! that writer sends, and the text on disk. A whole-file write refuses, and a
//! refusal costs the user the edit. This merge keeps both edits: it computes
//! the edit script of each side against the base, applies every cluster only
//! one side changed, and takes OURS in a cluster both sides changed
//! differently, reporting that cluster as a [`Region`] the user resolves.
//!
//! Ours wins provisionally because the caller is the writer: the text the
//! browser sends is the text its user is looking at. The region carries all
//! three texts, so no edit is lost and the user picks.
//!
//! An insert at the edge of the other side's change is not a conflict, and two
//! inserts at one point both survive, ours first. Two sides that made the same
//! change conflict with nothing.
//!
//! This module is pure: text in, text out, no filesystem. Its caller
//! normalises to LF and no BOM before it calls, the way
//! [`crate::note_edit`] expects its input.

use serde::{Deserialize, Serialize};
use similar::{capture_diff_slices, Algorithm, DiffOp};

/// A cluster both sides changed differently.
///
/// The lines are 1-based and the end is exclusive, the convention
/// `crate::session::types::review::LineRange` uses, and they point into the
/// MERGED text, so a reader can show the region without diffing again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Region {
    /// First line of the region in the merged text, 1-based, inclusive.
    pub start_line: u32,
    /// One past the last line of the region in the merged text, exclusive.
    /// Equal to `start_line` when our side deleted the cluster.
    pub end_line: u32,
    /// The cluster as the base held it.
    pub base: String,
    /// The cluster as our side wrote it. This is what the merged text holds.
    pub ours: String,
    /// The cluster as their side wrote it.
    pub theirs: String,
}

/// The merged text and every cluster the two sides disagree about.
///
/// An empty `regions` means the merge is clean and the caller may write
/// `text` with no question to the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Merge {
    pub text: String,
    pub regions: Vec<Region>,
}

/// One side's change to the base: the base lines `start..end` become `lines`.
/// An insert has `start == end`.
#[derive(Debug, Clone, Copy)]
struct Hunk<'a> {
    start: usize,
    end: usize,
    lines: &'a [&'a str],
}

impl Hunk<'_> {
    fn is_insert(&self) -> bool {
        self.start == self.end
    }

    /// Whether this hunk belongs in the cluster over base `cs..ce`. A cluster
    /// that is one insert point joins a hunk only when the point lies strictly
    /// inside it; two inserts at one point stay apart.
    fn joins(&self, cs: usize, ce: usize) -> bool {
        match (self.is_insert(), cs == ce) {
            (true, true) => false,
            (true, false) => cs < self.start && self.start < ce,
            (false, true) => self.start < cs && cs < self.end,
            (false, false) => self.start < ce && cs < self.end,
        }
    }
}

/// Merge `ours` and `theirs`, both derived from `base`, by line.
pub fn merge3(base: &str, ours: &str, theirs: &str) -> Merge {
    let base_lines: Vec<&str> = base.split_inclusive('\n').collect();
    let our_lines: Vec<&str> = ours.split_inclusive('\n').collect();
    let their_lines: Vec<&str> = theirs.split_inclusive('\n').collect();

    let our_hunks = hunks(&base_lines, &our_lines);
    let their_hunks = hunks(&base_lines, &their_lines);

    let mut out: Vec<&str> = Vec::new();
    let mut regions: Vec<Region> = Vec::new();
    let mut pos = 0;
    let (mut i, mut j) = (0, 0);

    while i < our_hunks.len() || j < their_hunks.len() {
        // Seed the cluster with the earliest hunk. Ours goes first on a tie,
        // EXCEPT when theirs is an insert at the point our change starts: the
        // insert never joins our cluster, so it must render before it. Seeded
        // the other way round, `pos` passes the insert and the slice below
        // panics.
        let (mut oi, mut tj) = (i, j);
        let seed = match (our_hunks.get(i), their_hunks.get(j)) {
            (Some(o), Some(t))
                if t.start < o.start || (t.start == o.start && t.is_insert() && !o.is_insert()) =>
            {
                tj += 1;
                t
            }
            (Some(o), _) => {
                oi += 1;
                o
            }
            (None, Some(t)) => {
                tj += 1;
                t
            }
            (None, None) => unreachable!("the loop condition holds one side"),
        };
        let (mut cs, mut ce) = (seed.start, seed.end);

        // Grow it while a hunk of either side overlaps its range.
        loop {
            let mut grew = false;
            while let Some(h) = our_hunks.get(oi).filter(|h| h.joins(cs, ce)) {
                cs = cs.min(h.start);
                ce = ce.max(h.end);
                oi += 1;
                grew = true;
            }
            while let Some(h) = their_hunks.get(tj).filter(|h| h.joins(cs, ce)) {
                cs = cs.min(h.start);
                ce = ce.max(h.end);
                tj += 1;
                grew = true;
            }
            if !grew {
                break;
            }
        }

        out.extend_from_slice(&base_lines[pos..cs]);
        let from_ours = &our_hunks[i..oi];
        let from_theirs = &their_hunks[j..tj];
        match (from_ours.is_empty(), from_theirs.is_empty()) {
            (false, true) => render(&base_lines, cs, ce, from_ours, &mut out),
            (true, false) => render(&base_lines, cs, ce, from_theirs, &mut out),
            (false, false) => {
                let mut ours_text = Vec::new();
                render(&base_lines, cs, ce, from_ours, &mut ours_text);
                let mut theirs_text = Vec::new();
                render(&base_lines, cs, ce, from_theirs, &mut theirs_text);
                // Ours goes in the merged text; a difference is a region the
                // user resolves, with all three texts to choose from.
                let start_line = out.len() as u32 + 1;
                out.extend_from_slice(&ours_text);
                let ours_joined = join(&ours_text);
                let theirs_joined = join(&theirs_text);
                if ours_joined != theirs_joined {
                    regions.push(Region {
                        start_line,
                        end_line: out.len() as u32 + 1,
                        base: join(&base_lines[cs..ce]),
                        ours: ours_joined,
                        theirs: theirs_joined,
                    });
                }
            }
            (true, true) => unreachable!("a cluster holds its seed"),
        }
        pos = ce;
        i = oi;
        j = tj;
    }
    out.extend_from_slice(&base_lines[pos..]);

    // A region's text is written back over exactly the lines it names, so it
    // must end them. `join` ends a line only when another line follows it in
    // the slice it is given, and a region is one such slice: a side that ends
    // without a newline ends the REGION, while the merged text may run on past
    // it. A region with a line after it therefore ends each of its texts, and a
    // region at the end of the note is left as the writers wrote it.
    let total = out.len() as u32;
    for region in &mut regions {
        if region.end_line <= total {
            terminate(&mut region.base);
            terminate(&mut region.ours);
            terminate(&mut region.theirs);
        }
    }

    Merge {
        text: join(&out),
        regions,
    }
}

/// The edit script of `new` against `base`, as hunks in base order.
fn hunks<'a>(base: &[&'a str], new: &'a [&'a str]) -> Vec<Hunk<'a>> {
    capture_diff_slices(Algorithm::Myers, base, new)
        .into_iter()
        .filter_map(|op| match op {
            DiffOp::Equal { .. } => None,
            DiffOp::Delete {
                old_index, old_len, ..
            } => Some(Hunk {
                start: old_index,
                end: old_index + old_len,
                lines: &[],
            }),
            DiffOp::Insert {
                old_index,
                new_index,
                new_len,
            } => Some(Hunk {
                start: old_index,
                end: old_index,
                lines: &new[new_index..new_index + new_len],
            }),
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => Some(Hunk {
                start: old_index,
                end: old_index + old_len,
                lines: &new[new_index..new_index + new_len],
            }),
        })
        .collect()
}

/// Join lines back into one text, ending any line that lost its newline.
///
/// `split_inclusive('\n')` leaves a text's LAST line without a newline, and a
/// merge may put that line in front of another side's lines. Concatenating
/// would glue two lines into one line neither writer wrote, so a line that is
/// not the last one here gets its newline back.
fn join(lines: &[&str]) -> String {
    let mut text = String::with_capacity(lines.iter().map(|l| l.len() + 1).sum());
    for (idx, line) in lines.iter().enumerate() {
        text.push_str(line);
        if idx + 1 < lines.len() && !line.ends_with('\n') {
            text.push('\n');
        }
    }
    text
}

/// Give `text` a final newline, unless it is empty.
///
/// An empty text is a side that deleted the cluster; it occupies no line, so a
/// newline would invent one.
fn terminate(text: &mut String) {
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
}

/// Base `cs..ce` with one side's sorted, disjoint `hunks` applied.
fn render<'a>(base: &[&'a str], cs: usize, ce: usize, hunks: &[Hunk<'a>], out: &mut Vec<&'a str>) {
    let mut pos = cs;
    for h in hunks {
        out.extend_from_slice(&base[pos..h.start]);
        out.extend_from_slice(h.lines);
        pos = h.end;
    }
    out.extend_from_slice(&base[pos..ce]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn changes_to_different_lines_both_apply() {
        let m = merge3("A\nB\nC\n", "A\nB\nC\nD\n", "A\nB2\nC\n");
        assert_eq!(m.text, "A\nB2\nC\nD\n");
        assert!(m.regions.is_empty());
    }

    #[test]
    fn a_cluster_both_sides_changed_takes_ours_and_reports_a_region() {
        let m = merge3("A\nB\nC\n", "A\nB-ours\nC\n", "A\nB-theirs\nC\n");
        assert_eq!(m.text, "A\nB-ours\nC\n");
        assert_eq!(
            m.regions,
            vec![Region {
                start_line: 2,
                end_line: 3,
                base: "B\n".to_string(),
                ours: "B-ours\n".to_string(),
                theirs: "B-theirs\n".to_string(),
            }]
        );
    }

    #[test]
    fn the_same_change_on_both_sides_is_no_region() {
        let m = merge3("A\nB\nC\n", "A\nB2\nC\n", "A\nB2\nC\n");
        assert_eq!(m.text, "A\nB2\nC\n");
        assert!(m.regions.is_empty());
    }

    #[test]
    fn an_unchanged_side_yields_the_other() {
        let m = merge3("A\nB\n", "A\nB\n", "X\nY\nZ\n");
        assert_eq!(m.text, "X\nY\nZ\n");
        assert!(m.regions.is_empty());

        let m = merge3("A\nB\n", "", "A\nB\n");
        assert_eq!(m.text, "");
        assert!(m.regions.is_empty());
    }

    #[test]
    fn an_insert_at_the_edge_of_a_changed_hunk_is_independent() {
        // Ours inserts before line 1; theirs rewrites line 1.
        let m = merge3("A\nB\n", "0\nA\nB\n", "A2\nB\n");
        assert_eq!(m.text, "0\nA2\nB\n");
        assert!(m.regions.is_empty());

        // Ours inserts after line 1; theirs rewrites line 1.
        let m = merge3("A\nB\n", "A\nA+\nB\n", "A2\nB\n");
        assert_eq!(m.text, "A2\nA+\nB\n");
        assert!(m.regions.is_empty());
    }

    #[test]
    fn two_inserts_at_one_point_both_survive() {
        let m = merge3("A\nB\n", "A\nours\nB\n", "A\ntheirs\nB\n");
        assert_eq!(m.text, "A\nours\ntheirs\nB\n");
        assert!(m.regions.is_empty());
    }

    #[test]
    fn a_delete_against_an_edit_of_the_same_line_takes_ours() {
        let m = merge3("A\nB\nC\n", "A\nC\n", "A\nB2\nC\n");
        assert_eq!(m.text, "A\nC\n");
        assert_eq!(
            m.regions,
            vec![Region {
                start_line: 2,
                end_line: 2,
                base: "B\n".to_string(),
                ours: String::new(),
                theirs: "B2\n".to_string(),
            }]
        );
    }

    #[test]
    fn overlapping_hunks_cluster_into_one_region() {
        // Ours rewrites B and C as one; theirs rewrites C and D as one.
        let m = merge3("A\nB\nC\nD\nE\n", "A\nX\nE\n", "A\nB\nY\nE\n");
        assert_eq!(m.text, "A\nX\nE\n");
        assert_eq!(m.regions.len(), 1);
        assert_eq!(m.regions[0].base, "B\nC\nD\n");
        assert_eq!(m.regions[0].ours, "X\n");
        assert_eq!(m.regions[0].theirs, "B\nY\n");
    }

    #[test]
    fn a_file_with_no_final_newline_merges() {
        let m = merge3("A\nB", "A\nB\nC", "A2\nB");
        assert_eq!(m.text, "A2\nB\nC");
        assert!(m.regions.is_empty());
    }

    /// `split_inclusive('\n')` lets a text's LAST line carry no newline, so a
    /// side that drops the final newline (or deletes the last line) must not
    /// glue its last line to whatever the other side appends after it.
    #[test]
    fn a_line_with_no_newline_never_glues_to_the_line_after_it() {
        let m = merge3("A\n", "A", "A\nB\n");
        assert_eq!(m.text, "A\nB\n");
        let m = merge3("A\n", "A\nB\n", "A");
        assert_eq!(m.text, "A\nB\n");
        let m = merge3("A\nB\nC\n", "A\nB", "A\nB\nC\nD\n");
        assert_eq!(m.text, "A\nB\nD\n");
    }

    /// The case the daemon's fold merge panics on: their insert sits at the
    /// first line of our change, so the insert never joins our cluster and the
    /// merge must still order the two.
    #[test]
    fn an_insert_by_theirs_at_the_start_of_our_hunk_merges_without_a_panic() {
        let m = merge3("A\nB\nC\n", "A\nB2\nC\n", "A\nX\nB\nC\n");
        assert_eq!(m.text, "A\nX\nB2\nC\n");
        assert!(m.regions.is_empty());
    }

    /// A region's text is the span of the merged text it names, byte for byte.
    ///
    /// A side that ends without a newline ends the merge only when nothing
    /// follows it. When the other side appends after the region, the merged
    /// text gives that line its newline back, and the region must say so: the
    /// browser writes the region's text back over exactly those lines, so a
    /// region that stops short glues its last line to the line after it.
    #[test]
    fn a_region_holds_the_lines_it_names_in_the_merged_text() {
        let cases = [
            ("A\nB\nC\n", "A\nO\nC", "A\nT\nC\nD\n"),
            ("A\nB\nC\n", "A\nO\nC\nD\n", "A\nT\nC"),
            ("A\nB\nC\n", "A\nX", "A\nY\nC\nD\n"),
        ];
        for (base, ours, theirs) in cases {
            let m = merge3(base, ours, theirs);
            assert_eq!(
                m.regions.len(),
                1,
                "one region for {ours:?} against {theirs:?}"
            );
            let region = &m.regions[0];
            let lines: Vec<&str> = m.text.split_inclusive('\n').collect();
            let span = lines[region.start_line as usize - 1..region.end_line as usize - 1].concat();
            assert_eq!(span, region.ours, "ours is what the merged text holds");
            assert!(
                region.theirs.is_empty() || region.theirs.ends_with('\n'),
                "theirs ends where the merged text carries on: {:?}",
                region.theirs
            );
        }
    }

    /// The merged text runs on past the region, so the text a choice writes
    /// back over the region's lines must end the last of them.
    #[test]
    fn a_region_that_shortens_the_note_still_ends_its_last_line() {
        let m = merge3("A\nB\nC\n", "A\nX", "A\nY\nC\nD\n");
        assert_eq!(m.text, "A\nX\nD\n");
        assert_eq!(m.regions.len(), 1);
        assert_eq!(m.regions[0].start_line, 2);
        assert_eq!(m.regions[0].end_line, 3);
        assert_eq!(m.regions[0].ours, "X\n");
        assert_eq!(m.regions[0].theirs, "Y\nC\n");
    }

    /// A region at the END of the merged text has no line after it, so neither
    /// side gains a newline it did not have, and a deleted side stays empty.
    #[test]
    fn a_region_at_the_end_of_the_note_keeps_its_last_line_unended() {
        let m = merge3("A\nB\n", "A\nO", "A\nT");
        assert_eq!(m.text, "A\nO");
        assert_eq!(m.regions.len(), 1);
        assert_eq!(m.regions[0].ours, "O");
        assert_eq!(m.regions[0].theirs, "T");
    }

    #[test]
    fn region_lines_point_into_the_merged_text() {
        // Two conflicting clusters, with an unchanged line between them.
        let m = merge3(
            "A\nB\nC\nD\nE\n",
            "A\nB-ours\nC\nD-ours\nE\n",
            "A\nB-theirs\nC\nD-theirs\nE\n",
        );
        assert_eq!(m.text, "A\nB-ours\nC\nD-ours\nE\n");
        assert_eq!(m.regions.len(), 2);
        let lines: Vec<&str> = m.text.split_inclusive('\n').collect();
        for region in &m.regions {
            let start = region.start_line as usize - 1;
            let end = region.end_line as usize - 1;
            assert_eq!(lines[start..end].concat(), region.ours);
        }
        assert_eq!(m.regions[0].start_line, 2);
        assert_eq!(m.regions[1].start_line, 4);
    }

    /// A base text and one side's edit of it: every line is kept, rewritten,
    /// dropped, or preceded by a new line. Two independent line lists share no
    /// context, so the merge would never have to carry an unchanged tail.
    fn a_base_and_an_edit_of_it() -> impl Strategy<Value = (String, String)> {
        prop::collection::vec(("[a-z]{1,4}", 0u8..4), 0..8).prop_map(|rows| {
            let mut base = String::new();
            let mut edit = String::new();
            for (line, op) in rows {
                base.push_str(&line);
                base.push('\n');
                match op {
                    0 => {
                        edit.push_str(&line);
                        edit.push('\n');
                    }
                    1 => edit.push_str("rewritten\n"),
                    2 => {}
                    _ => {
                        edit.push_str("added\n");
                        edit.push_str(&line);
                        edit.push('\n');
                    }
                }
            }
            (base, edit)
        })
    }

    proptest! {
        /// A side that made no change never changes the answer.
        #[test]
        fn a_side_equal_to_the_base_yields_the_other_side(
            (base, other) in a_base_and_an_edit_of_it(),
        ) {
            let ours = merge3(&base, &other, &base);
            prop_assert_eq!(&ours.text, &other);
            prop_assert!(ours.regions.is_empty());

            let theirs = merge3(&base, &base, &other);
            prop_assert_eq!(&theirs.text, &other);
            prop_assert!(theirs.regions.is_empty());
        }
    }
}
