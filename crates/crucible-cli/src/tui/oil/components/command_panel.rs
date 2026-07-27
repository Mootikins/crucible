//! CommandPanel: the prompt region — TurnIndicator plus the authored list.
//!
//! The region is one ordered list that contains the input, so the panel does
//! not decide what sits above or below it; it renders what the author wrote, in
//! the order they wrote it.

use crate::tui::oil::app::ViewContext;
use crate::tui::oil::component::Component;
use crate::tui::oil::components::{InputComponent, StatusComponent, TurnIndicator};
use crucible_lua::statusline_items::Region;
use crucible_oil::node::{col, Node};
use crucible_oil::style::Gap;

pub struct CommandPanel<'a> {
    pub turn_indicator: TurnIndicator,
    pub input: InputComponent<'a>,
    pub status: StatusComponent<'a>,
}

impl Component for CommandPanel<'_> {
    fn view(&self, ctx: &ViewContext<'_>) -> Node {
        // The turn indicator is chrome the author does not place: it appears
        // and disappears mid-turn, and putting it inside the authored list
        // would shift every row underneath it each time a turn starts.
        let rows = self
            .status
            .render_region(Region::Prompt, || self.input.view(ctx));

        col([self.turn_indicator.view(ctx), col(rows)]).gap(Gap::row(1))
    }
}
