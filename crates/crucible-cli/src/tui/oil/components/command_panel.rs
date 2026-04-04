//! CommandPanel: footer chrome composing TurnIndicator + InputComponent + StatusComponent.

use crate::tui::oil::app::ViewContext;
use crate::tui::oil::component::Component;
use crate::tui::oil::components::{InputComponent, StatusComponent, TurnIndicator};
use crucible_oil::node::{col, Node};
use crucible_oil::style::Gap;

pub struct CommandPanel<'a> {
    pub turn_indicator: TurnIndicator,
    pub input: InputComponent<'a>,
    pub status: StatusComponent<'a>,
}

impl Component for CommandPanel<'_> {
    fn view(&self, ctx: &ViewContext<'_>) -> Node {
        col([
            self.turn_indicator.view(ctx),
            col([self.input.view(ctx), self.status.view(ctx)]),
        ])
        .gap(Gap::row(1))
    }
}
