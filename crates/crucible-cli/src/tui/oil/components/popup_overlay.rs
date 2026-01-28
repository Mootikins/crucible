pub use crucible_oil::components::{
    popup_item, popup_item_full, popup_item_with_desc, PopupOverlay, FOCUS_POPUP, POPUP_MAX_VISIBLE,
};

use crate::tui::oil::component::Component;
use crate::tui::oil::node::Node;
use crate::tui::oil::ViewContext;

impl Component for PopupOverlay {
    fn view(&self, ctx: &ViewContext<'_>) -> Node {
        self.view(ctx.focus)
    }
}
