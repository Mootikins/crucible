//! CommandPanel: footer chrome composing TurnIndicator + InputComponent + StatusComponent.

use crate::tui::oil::app::ViewContext;
use crate::tui::oil::component::Component;
use crate::tui::oil::components::{InputComponent, StatusComponent, TurnIndicator};
use crucible_lua::statusline_items::Anchor;
use crucible_oil::node::{col, Node};
use crucible_oil::style::Gap;

pub struct CommandPanel<'a> {
    pub turn_indicator: TurnIndicator,
    pub input: InputComponent<'a>,
    pub status: StatusComponent<'a>,
}

impl Component for CommandPanel<'_> {
    fn view(&self, ctx: &ViewContext<'_>) -> Node {
        // `footer.above_input` sits inside the inner column, not before the turn
        // indicator — a bar the author placed above the input should hug it, and
        // stay put when the indicator appears and disappears mid-turn.
        let mut inner = self.status.views_at(Anchor::FooterAboveInput);
        inner.push(self.input.view(ctx));
        inner.push(self.status.view(ctx));

        col([self.turn_indicator.view(ctx), col(inner)]).gap(Gap::row(1))
    }
}
