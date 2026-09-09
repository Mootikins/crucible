/**
 * The wire shape of a serialized Oil node tree.
 *
 * Owned by Rust: `crucible-oil`'s `Node` derives this, and
 * `crates/crucible-oil/tests/wire_shape.rs` pins it. Deliberately loose here —
 * an externally-tagged enum with ten variants and defaults omitted does not
 * gain safety from being restated in TypeScript, it gains a second definition
 * to keep in sync. The renderer switches on the tag and degrades on an
 * unknown one, which is the check that actually holds.
 *
 * A bare string is `Node::Empty` — a unit variant of an externally tagged enum.
 */
export type OilTree = string | Record<string, unknown>;
