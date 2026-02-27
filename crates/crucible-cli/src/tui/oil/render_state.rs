#[derive(Debug, Clone, Copy)]
pub struct RenderState {
    pub terminal_width: u16,
    pub spinner_frame: usize,
    pub show_thinking: bool,
}

impl RenderState {
    #[inline]
    pub fn width(self) -> usize {
        self.terminal_width as usize
    }
}
