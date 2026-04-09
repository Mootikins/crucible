#[allow(unused_imports)] // WIP: popup_item_full not yet used
pub use crucible_oil::components::{
    popup_item, popup_item_full, popup_item_with_desc, PopupOverlay, FOCUS_POPUP, POPUP_MAX_VISIBLE,
};

use crate::tui::oil::component::Component;
use crate::tui::oil::ViewContext;
use crucible_oil::node::Node;

impl Component for PopupOverlay {
    fn view(&self, ctx: &ViewContext<'_>) -> Node {
        self.view(ctx.focus)
    }
}
