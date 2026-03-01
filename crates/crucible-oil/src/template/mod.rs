mod html;
mod node_spec;

pub use html::{html_to_node, HtmlError, HtmlResult};
pub use node_spec::{
    parse_color, spec_to_node, NodeAttrs, NodeSpec, NodeSpecError, NodeSpecResult,
};
