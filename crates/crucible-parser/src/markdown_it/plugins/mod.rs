//! Custom markdown-it plugins for Obsidian-style syntax

pub mod callout;
pub mod latex;
pub mod tag;
pub mod wikilink;

pub use callout::add_callout_plugin;
pub use latex::add_latex_plugin;
pub use tag::add_tag_plugin;
pub use wikilink::add_wikilink_plugin;
