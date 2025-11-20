//! Custom markdown-it plugins for Obsidian-style syntax

pub mod wikilink;
pub mod tag;
// pub mod callout; // TODO: Fix block rule API
pub mod latex;

pub use wikilink::add_wikilink_plugin;
pub use tag::add_tag_plugin;
// pub use callout::add_callout_plugin; // TODO: Fix block rule API
pub use latex::add_latex_plugin;
