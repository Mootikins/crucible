mod html;
mod node_spec;

pub use html::{HtmlError, HtmlResult, html_to_node};
pub use node_spec::{NodeSpec, NodeAttrs, NodeSpecError, NodeSpecResult, spec_to_node, parse_color};
