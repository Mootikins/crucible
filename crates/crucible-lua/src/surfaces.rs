//! Surfaces a plugin declares for every client to draw.
//!
//! `cru.surface.declare{...}` gives a plugin a named panel — a session list, a
//! review queue, a kiln tree — that the TUI and the web each render in their own
//! idiom. The plugin states *what* it has; neither client is told *how* to draw
//! it, and neither learns what the plugin is about.
//!
//! # Why data and not a node tree
//!
//! `cru.oil.*` can already build a terminal node tree, and nothing consumes one.
//! Making that tree the cross-client contract was the first design and it was
//! wrong: a cell grid in a browser forfeits DOM roles and labels for screen
//! readers, real text inputs, find and select, reflow under zoom, native scroll
//! and pixels. DOM can express a grid; a grid can never express DOM. So the
//! layer both media share is semantic — "a list of rows with these fields" —
//! and a node tree stays a terminal-only escape hatch.
//!
//! # Why the vocabulary is closed and the field set is not
//!
//! The statusline is the warning: a closed *layout* vocabulary
//! (`Region`/`Element`/`Layout`) needs bespoke drawing per member per client, so
//! it stalled at one renderer and the web draws none of it. The rule that works
//! instead comes from Home Assistant, which holds 38 card types: variation lands
//! in a **field** of an existing shape, never in a new shape. So a row's status
//! is a [`Mark`] field with a stated meaning, not a glyph the plugin chooses and
//! each client guesses at.
//!
//! # Caps and sanitisation live here
//!
//! At the registry boundary, not in a renderer. A row reaches two clients and a
//! persisted layout, and the terminal overlay renderer parses ANSI out of plain
//! strings, so an unsanitised row is an injection path into the terminal. This
//! is the same reason the statusline sanitises on the way in.

use mlua::{Lua, Table};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// What the registry calls when a surface changes.
///
/// A closure rather than a channel, and `dyn` rather than a generic, because
/// this is a **crate-dependency firewall**: the daemon owns sequence stamping
/// on its broadcast bus, and a registry that sent to that bus directly would
/// emit unstamped events that break a client's gap detection. So the daemon
/// installs a closure that calls its own emitter, and this crate never learns
/// what a bus is.
pub type SurfaceEmitter = Arc<dyn Fn(SurfaceChange) + Send + Sync>;

/// What changed, for the daemon to put on the wire.
///
/// Deliberately not the rows. A surface is unbounded where an event is not, and
/// two clients want it at different times, so the event says what moved and each
/// client asks for the content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceChange {
    pub plugin: String,
    pub name: String,
    pub version: u64,
    pub session: Option<String>,
}

/// Longest title kept, in characters.
pub const MAX_TITLE_CHARS: usize = 64;
/// Longest row text or detail kept, in characters.
pub const MAX_TEXT_CHARS: usize = 200;
/// Most rows kept for one surface.
///
/// A surface is a panel a person reads, not a log. A plugin that pushes more is
/// truncated rather than refused: losing the tail of a list is better than
/// losing the list.
pub const MAX_ROWS: usize = 500;

/// What a client draws.
///
/// One variant, because one renderer exists. `Tree`, `Table` and `KeyValue` are
/// named in the design and arrive **with** their renderers — the exhaustive
/// match on this enum is what forces that, and a variant added ahead of its
/// renderer would be a shape a plugin can declare and no client can draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// Rows in order, one line each.
    List,
}

impl Shape {
    /// The name a plugin writes, and the name on the wire.
    ///
    /// **No wildcard arm, ever.** A new shape must fail to compile until
    /// someone names it here and in both renderers.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::List => "list",
        }
    }

    /// The shape for a declared name, or `None` when nothing draws it.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "list" => Some(Self::List),
            _ => None,
        }
    }
}

/// A row's status, stated semantically so each client picks its own glyph.
///
/// The plugin says what is true; the TUI may draw `●` and the web a coloured
/// dot. A plugin that shipped its own glyph would bind one client's medium into
/// a contract both must honour, which is the mistake this enum exists to
/// prevent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    /// Work is underway.
    Busy,
    /// Waiting on a person.
    Blocked,
    /// Finished, nothing wrong.
    Ok,
    /// Finished, something is wrong.
    Failed,
}

impl Mark {
    /// **No wildcard arm, ever** — same reason as [`Shape::as_str`].
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Busy => "busy",
            Self::Blocked => "blocked",
            Self::Ok => "ok",
            Self::Failed => "failed",
        }
    }

    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "busy" => Some(Self::Busy),
            "blocked" => Some(Self::Blocked),
            "ok" => Some(Self::Ok),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

/// One row of a surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceRow {
    /// Stable identity, chosen by the plugin. What an action names later, and
    /// what a client keys a selection on across a re-push.
    pub id: String,
    /// The row's own text.
    pub text: String,
    /// Secondary text, when the row has any.
    pub detail: Option<String>,
    /// Status, when the row has one. `None` is "no status", never "unknown".
    pub mark: Option<Mark>,
}

/// One declared surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Surface {
    /// The plugin that declared it, so a stale surface can be attributed.
    pub plugin: String,
    /// The plugin's own name for it. Stable across a reload — see
    /// [`SurfaceRegistry`].
    pub name: String,
    pub title: String,
    pub shape: Shape,
    /// The session this surface is about, or `None` when it is about the plugin.
    pub session: Option<String>,
    pub rows: Vec<SurfaceRow>,
    /// Bumped on every row change, so a client redraws on a change it sees
    /// rather than on a timer.
    pub version: u64,
}

/// Declared surfaces, keyed by `(plugin, name)`.
///
/// **Keyed on the plugin's own name, never on a generated id.** A reload calls
/// `make_plugin_inert`, which releases the publication and option registries; a
/// generated id would therefore change on every reload and every persisted
/// window naming it would dangle — the failure the web's
/// `layoutRestore.dangling.test.ts` exists to cover. Re-declaration after a
/// reload lands on the same key, so a client's window still resolves.
///
/// Like `StatusRegistry`, and unlike publications, there is no release on the
/// inert path: a reload is meant to be invisible. Removing a plugin for good is
/// [`Self::release_plugin`], called from an uninstall rather than from a reload.
#[derive(Clone, Default)]
pub struct SurfaceRegistry {
    entries: Arc<Mutex<HashMap<(String, String), Surface>>>,
    /// Installed once by the daemon, after the Lua module is registered.
    ///
    /// Shared through the `Arc` rather than held per clone: the closure Lua
    /// captured holds its own clone of this registry, so an emitter stored on
    /// one clone would never be seen by the writer that matters.
    emitter: Arc<OnceLock<SurfaceEmitter>>,
}

impl std::fmt::Debug for SurfaceRegistry {
    /// Hand-written because a closure has no `Debug`. Reports whether an
    /// emitter is installed, which is the thing worth knowing when a client is
    /// not redrawing.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SurfaceRegistry")
            .field(
                "surfaces",
                &self.entries.lock().map(|e| e.len()).unwrap_or(0),
            )
            .field("emits", &self.emitter.get().is_some())
            .finish()
    }
}

impl SurfaceRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Install the change emitter, once. Returns `false` if one was already
    /// installed, which is a double-boot rather than something to paper over.
    ///
    /// Takes `&self` because every clone shares the slot. See
    /// [`SurfaceEmitter`] for why it is a closure.
    pub fn set_emitter(&self, emitter: SurfaceEmitter) -> bool {
        self.emitter.set(emitter).is_ok()
    }

    /// Report a change, if anything is listening.
    fn announce(&self, surface: &Surface) {
        if let Some(emitter) = self.emitter.get() {
            emitter(SurfaceChange {
                plugin: surface.plugin.clone(),
                name: surface.name.clone(),
                version: surface.version,
                session: surface.session.clone(),
            });
        }
    }

    /// Declare a surface, or update the metadata of one already declared.
    ///
    /// Idempotent on purpose. A reload re-runs `init.lua`, so this is the call
    /// that must not lose what a client is pointing at: an existing surface
    /// keeps its rows and its version, and only its title, shape and session
    /// are refreshed. A plugin that wants the rows gone calls
    /// [`Self::set_rows`] with none.
    pub fn declare(
        &self,
        plugin: &str,
        name: &str,
        title: &str,
        shape: Shape,
        session: Option<String>,
    ) {
        let key = (plugin.to_string(), name.to_string());
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        match entries.get_mut(&key) {
            Some(existing) => {
                existing.title = cap_title(title);
                existing.shape = shape;
                existing.session = session;
            }
            None => {
                entries.insert(
                    key,
                    Surface {
                        plugin: plugin.to_string(),
                        name: name.to_string(),
                        title: cap_title(title),
                        shape,
                        session,
                        rows: Vec::new(),
                        version: 0,
                    },
                );
            }
        }
    }

    /// Replace a surface's rows and bump its version.
    ///
    /// Silent on an undeclared surface: `declare` is what creates one, and
    /// guessing a title and a shape here would let a typo in a name produce a
    /// second, untitled surface.
    pub fn set_rows(&self, plugin: &str, name: &str, rows: Vec<SurfaceRow>) {
        let key = (plugin.to_string(), name.to_string());
        // The announce happens after the lock is released: an emitter that
        // reaches a client synchronously must not hold the registry while it
        // does, or a handler that reads a surface deadlocks against the write
        // that woke it.
        let changed = {
            let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
            entries.get_mut(&key).map(|surface| {
                surface.rows = rows.into_iter().take(MAX_ROWS).map(cap_row).collect();
                surface.version = surface.version.saturating_add(1);
                surface.clone()
            })
        };
        if let Some(surface) = changed {
            self.announce(&surface);
        }
    }

    /// One surface, by plugin and name.
    #[must_use]
    pub fn get(&self, plugin: &str, name: &str) -> Option<Surface> {
        let key = (plugin.to_string(), name.to_string());
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&key)
            .cloned()
    }

    /// Every surface, in a stable order so a client's list does not reshuffle.
    #[must_use]
    pub fn list(&self) -> Vec<Surface> {
        let mut all: Vec<Surface> = self
            .entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .cloned()
            .collect();
        all.sort_by(|a, b| (&a.plugin, &a.name).cmp(&(&b.plugin, &b.name)));
        all
    }

    /// Drop one surface.
    pub fn remove(&self, plugin: &str, name: &str) {
        let key = (plugin.to_string(), name.to_string());
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&key);
    }

    /// Drop every surface a plugin declared.
    ///
    /// For an uninstall, **not** for a reload. A reload that called this would
    /// orphan every window pointing at the surfaces it is about to re-declare.
    pub fn release_plugin(&self, plugin: &str) {
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|(p, _), _| p != plugin);
    }
}

/// Strip display-hostile characters and cap a title.
fn cap_title(value: &str) -> String {
    crucible_core::text::sanitize_single_line(value)
        .chars()
        .take(MAX_TITLE_CHARS)
        .collect()
}

/// Strip display-hostile characters and cap one field.
fn cap_text(value: &str) -> String {
    crucible_core::text::sanitize_single_line(value)
        .chars()
        .take(MAX_TEXT_CHARS)
        .collect()
}

/// Sanitise and cap every string a row carries.
///
/// `id` is capped too. It is chosen by the plugin, but it reaches a client as
/// text and a persisted selection, so it is not exempt.
fn cap_row(row: SurfaceRow) -> SurfaceRow {
    SurfaceRow {
        id: cap_text(&row.id),
        text: cap_text(&row.text),
        detail: row.detail.as_deref().map(cap_text),
        mark: row.mark,
    }
}

/// Read one row out of a Lua table.
///
/// An unknown `mark` is dropped rather than refused: a plugin naming a status
/// this build has no vocabulary for should still show its row.
fn row_from_table(t: &Table) -> mlua::Result<SurfaceRow> {
    let id: String = t
        .get("id")
        .map_err(|_| mlua::Error::runtime("cru.surface: every row needs an `id`"))?;
    let text: String = t.get("text").unwrap_or_else(|_| id.clone());
    let detail: Option<String> = t.get("detail").ok();
    let mark = t
        .get::<Option<String>>("mark")
        .ok()
        .flatten()
        .and_then(|m| Mark::parse(&m));
    Ok(SurfaceRow {
        id,
        text,
        detail,
        mark,
    })
}

/// Register `cru.surface.*`.
pub fn register_surface_module(
    lua: &Lua,
    registry: SurfaceRegistry,
) -> Result<(), crate::error::LuaError> {
    let surface = crate::lua_util::get_or_create_module(lua, "surface")?;
    let mut ns = crate::host_registry::Ns::over(lua, "cru.surface", surface);

    let declare_registry = registry.clone();
    ns.func(
        "declare",
        "(surface: { name: string, title: string?, shape: string?, plugin: string?, \
         session: string? }) -> ()",
        move |_, opts: Table| {
            let name: String = opts
                .get("name")
                .map_err(|_| mlua::Error::runtime("cru.surface.declare: `name` is required"))?;
            let title: String = opts.get("title").unwrap_or_else(|_| name.clone());
            let plugin: String = opts.get("plugin").unwrap_or_else(|_| "unknown".to_string());
            let session: Option<String> = opts.get("session").ok().flatten();
            let shape_name: String = opts.get("shape").unwrap_or_else(|_| "list".to_string());
            let shape = Shape::parse(&shape_name).ok_or_else(|| {
                mlua::Error::runtime(format!(
                    "cru.surface.declare: no client draws shape `{shape_name}`"
                ))
            })?;
            declare_registry.declare(&plugin, &name, &title, shape, session);
            Ok(())
        },
    )?;

    let rows_registry = registry;
    ns.func(
        "set_rows",
        "(surface: { name: string, plugin: string?, rows: { { id: string, text: string?, \
         detail: string?, mark: string? } } }) -> ()",
        move |_, opts: Table| {
            let name: String = opts
                .get("name")
                .map_err(|_| mlua::Error::runtime("cru.surface.set_rows: `name` is required"))?;
            let plugin: String = opts.get("plugin").unwrap_or_else(|_| "unknown".to_string());
            let rows: Table = opts
                .get("rows")
                .map_err(|_| mlua::Error::runtime("cru.surface.set_rows: `rows` is required"))?;
            let mut out = Vec::new();
            for entry in rows.sequence_values::<Table>() {
                out.push(row_from_table(&entry?)?);
            }
            rows_registry.set_rows(&plugin, &name, out);
            Ok(())
        },
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str) -> SurfaceRow {
        SurfaceRow {
            id: id.to_string(),
            text: id.to_string(),
            detail: None,
            mark: None,
        }
    }

    fn declared() -> SurfaceRegistry {
        let reg = SurfaceRegistry::new();
        reg.declare("p", "sessions", "Sessions", Shape::List, None);
        reg
    }

    #[test]
    fn set_rows_bumps_the_version() {
        let reg = declared();
        assert_eq!(reg.get("p", "sessions").unwrap().version, 0);
        reg.set_rows("p", "sessions", vec![row("a")]);
        assert_eq!(reg.get("p", "sessions").unwrap().version, 1);
        reg.set_rows("p", "sessions", vec![row("a"), row("b")]);
        assert_eq!(reg.get("p", "sessions").unwrap().version, 2);
    }

    /// A reload re-runs `init.lua`, so `declare` runs again on a live surface.
    /// It must not reset what a client is already pointing at.
    #[test]
    fn redeclaring_keeps_the_rows_and_the_version() {
        let reg = declared();
        reg.set_rows("p", "sessions", vec![row("a"), row("b")]);

        reg.declare("p", "sessions", "Sessions renamed", Shape::List, None);

        let after = reg.get("p", "sessions").expect("the surface survives");
        assert_eq!(after.rows.len(), 2, "a reload kept the rows");
        assert_eq!(after.version, 1, "a reload did not bump the version");
        assert_eq!(after.title, "Sessions renamed", "metadata still refreshes");
    }

    /// The key is the plugin's own name, so a client's window resolves to the
    /// same surface after a reload. A generated id would not.
    #[test]
    fn the_key_is_stable_across_a_redeclare() {
        let reg = declared();
        let before = reg.get("p", "sessions").unwrap().name;
        reg.declare("p", "sessions", "Sessions", Shape::List, None);
        assert_eq!(before, reg.get("p", "sessions").unwrap().name);
        assert_eq!(reg.list().len(), 1, "a redeclare did not make a second one");
    }

    #[test]
    fn rows_are_capped() {
        let reg = declared();
        let many: Vec<SurfaceRow> = (0..MAX_ROWS + 50).map(|i| row(&i.to_string())).collect();
        reg.set_rows("p", "sessions", many);
        assert_eq!(reg.get("p", "sessions").unwrap().rows.len(), MAX_ROWS);
    }

    #[test]
    fn row_text_is_capped() {
        let reg = declared();
        reg.set_rows(
            "p",
            "sessions",
            vec![SurfaceRow {
                id: "a".into(),
                text: "x".repeat(MAX_TEXT_CHARS + 100),
                detail: Some("y".repeat(MAX_TEXT_CHARS + 100)),
                mark: None,
            }],
        );
        let rows = reg.get("p", "sessions").unwrap().rows;
        assert_eq!(rows[0].text.chars().count(), MAX_TEXT_CHARS);
        assert_eq!(
            rows[0].detail.as_ref().unwrap().chars().count(),
            MAX_TEXT_CHARS
        );
    }

    #[test]
    fn the_title_is_capped() {
        let reg = SurfaceRegistry::new();
        reg.declare(
            "p",
            "s",
            &"t".repeat(MAX_TITLE_CHARS + 20),
            Shape::List,
            None,
        );
        assert_eq!(
            reg.get("p", "s").unwrap().title.chars().count(),
            MAX_TITLE_CHARS
        );
    }

    /// A row reaches the terminal, whose overlay renderer parses ANSI out of
    /// plain strings. An escape in a row is therefore an injection, and this is
    /// the boundary that has to stop it.
    #[test]
    fn an_escape_does_not_survive_a_row() {
        let reg = declared();
        reg.set_rows(
            "p",
            "sessions",
            vec![SurfaceRow {
                id: "a\x1b[31m".into(),
                text: "red\x1b[2J".into(),
                detail: Some("\x07bell".into()),
                mark: None,
            }],
        );
        let rows = reg.get("p", "sessions").unwrap().rows;
        for field in [
            rows[0].id.as_str(),
            rows[0].text.as_str(),
            rows[0].detail.as_deref().unwrap(),
        ] {
            assert!(
                !field.contains('\x1b') && !field.contains('\x07'),
                "an escape survived into `{field:?}`"
            );
        }
    }

    #[test]
    fn an_escape_does_not_survive_a_title() {
        let reg = SurfaceRegistry::new();
        reg.declare("p", "s", "Ses\x1b[31msions", Shape::List, None);
        assert!(!reg.get("p", "s").unwrap().title.contains('\x1b'));
    }

    #[test]
    fn set_rows_on_an_undeclared_surface_creates_nothing() {
        let reg = SurfaceRegistry::new();
        reg.set_rows("p", "typo", vec![row("a")]);
        assert!(reg.list().is_empty());
    }

    #[test]
    fn release_plugin_drops_only_that_plugins_surfaces() {
        let reg = SurfaceRegistry::new();
        reg.declare("a", "one", "One", Shape::List, None);
        reg.declare("b", "two", "Two", Shape::List, None);
        reg.release_plugin("a");
        let left = reg.list();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].plugin, "b");
    }

    #[test]
    fn a_shape_round_trips_and_an_unknown_one_is_refused() {
        assert_eq!(Shape::parse(Shape::List.as_str()), Some(Shape::List));
        assert_eq!(Shape::parse("treemap"), None);
    }

    fn lua_with_surfaces(reg: SurfaceRegistry) -> Lua {
        let lua = Lua::new();
        register_surface_module(&lua, reg).unwrap();
        lua
    }

    /// The Rust-to-Lua boundary, crossed once. Everything above tests the
    /// registry directly; this proves a plugin author's own call reaches it.
    #[test]
    fn a_plugin_declares_and_fills_a_surface_from_lua() {
        let reg = SurfaceRegistry::new();
        let lua = lua_with_surfaces(reg.clone());

        lua.load(
            r#"
            cru.surface.declare{ plugin="p", name="sessions", title="Sessions" }
            cru.surface.set_rows{ plugin="p", name="sessions", rows={
                { id="s1", text="crucible", mark="busy" },
                { id="s2", text="web-fix", mark="blocked", detail="waiting" },
                { id="s3" },
            } }
        "#,
        )
        .exec()
        .expect("the Lua calls succeed");

        let surface = reg.get("p", "sessions").expect("declared from Lua");
        assert_eq!(surface.title, "Sessions");
        assert_eq!(surface.shape, Shape::List);
        assert_eq!(surface.version, 1);
        assert_eq!(surface.rows.len(), 3);
        assert_eq!(surface.rows[0].mark, Some(Mark::Busy));
        assert_eq!(surface.rows[1].detail.as_deref(), Some("waiting"));
        assert_eq!(
            surface.rows[2].text, "s3",
            "a row with no text falls back to its id"
        );
        assert_eq!(surface.rows[2].mark, None);
    }

    /// A shape no client draws is refused at the call, not stored and skipped
    /// later. A stored-but-undrawable surface is an empty panel with no cause.
    #[test]
    fn lua_refuses_a_shape_no_client_draws() {
        let reg = SurfaceRegistry::new();
        let lua = lua_with_surfaces(reg.clone());
        let err = lua
            .load(r#"cru.surface.declare{ name="x", shape="treemap" }"#)
            .exec()
            .expect_err("an undrawable shape is refused");
        assert!(
            err.to_string().contains("treemap"),
            "the error names the shape: {err}"
        );
        assert!(reg.list().is_empty(), "nothing was stored");
    }

    /// A row with no `id` is refused: `id` is what an action names and what a
    /// client keys a selection on, so a row without one cannot be acted upon.
    #[test]
    fn lua_refuses_a_row_with_no_id() {
        let reg = SurfaceRegistry::new();
        let lua = lua_with_surfaces(reg.clone());
        lua.load(r#"cru.surface.declare{ plugin="p", name="s" }"#)
            .exec()
            .unwrap();
        let err = lua
            .load(r#"cru.surface.set_rows{ plugin="p", name="s", rows={ { text="no id" } } }"#)
            .exec()
            .expect_err("a row with no id is refused");
        assert!(err.to_string().contains("id"), "{err}");
        assert!(reg.get("p", "s").unwrap().rows.is_empty());
    }

    /// The emitter is what makes a client redraw. A `set_rows` that did not
    /// announce would leave every client showing the previous rows until
    /// something else happened to wake it.
    #[test]
    fn set_rows_announces_the_change() {
        let seen: Arc<Mutex<Vec<SurfaceChange>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        let reg = SurfaceRegistry::new();
        assert!(reg.set_emitter(Arc::new(move |change| {
            sink.lock().unwrap().push(change);
        })));

        reg.declare("p", "sessions", "Sessions", Shape::List, Some("s1".into()));
        assert!(
            seen.lock().unwrap().is_empty(),
            "a declare has no rows to report yet"
        );

        reg.set_rows("p", "sessions", vec![row("a")]);

        let changes = seen.lock().unwrap();
        assert_eq!(changes.len(), 1, "one row change, one announcement");
        assert_eq!(changes[0].plugin, "p");
        assert_eq!(changes[0].name, "sessions");
        assert_eq!(
            changes[0].version, 1,
            "the announcement carries the version"
        );
        assert_eq!(
            changes[0].session.as_deref(),
            Some("s1"),
            "and the session the surface is about"
        );
    }

    /// An emitter is installed once. A second install is a double boot, which
    /// should be visible rather than silently replacing the first.
    #[test]
    fn the_emitter_installs_once() {
        let reg = SurfaceRegistry::new();
        assert!(reg.set_emitter(Arc::new(|_| {})));
        assert!(!reg.set_emitter(Arc::new(|_| {})), "the second is refused");
    }

    /// Every clone shares the slot. The closure Lua captured holds its own
    /// clone, so an emitter that lived per clone would never fire for the write
    /// that matters.
    #[test]
    fn a_clone_sees_the_emitter() {
        let hits = Arc::new(Mutex::new(0usize));
        let sink = Arc::clone(&hits);
        let reg = SurfaceRegistry::new();
        reg.declare("p", "s", "S", Shape::List, None);
        reg.set_emitter(Arc::new(move |_| *sink.lock().unwrap() += 1));

        let clone = reg.clone();
        clone.set_rows("p", "s", vec![row("a")]);

        assert_eq!(*hits.lock().unwrap(), 1, "the clone announced");
    }

    #[test]
    fn every_mark_round_trips() {
        for mark in [Mark::Busy, Mark::Blocked, Mark::Ok, Mark::Failed] {
            assert_eq!(Mark::parse(mark.as_str()), Some(mark), "{mark:?}");
        }
        assert_eq!(Mark::parse("sideways"), None);
    }
}
