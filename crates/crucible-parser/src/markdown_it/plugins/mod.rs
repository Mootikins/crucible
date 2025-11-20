//! Custom markdown-it plugins for Obsidian-style syntax

pub mod wikilink;
pub mod tag;
pub mod callout;
pub mod latex;

pub use wikilink::add_wikilink_plugin;
pub use tag::add_tag_plugin;
pub use callout::add_callout_plugin;
pub use latex::add_latex_plugin;
