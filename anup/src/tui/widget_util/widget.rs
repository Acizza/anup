use tui::widgets::{Paragraph, Wrap};

pub trait WrapHelper {
    fn wrapped(self) -> Self;
}

impl<'a> WrapHelper for Paragraph<'a> {
    fn wrapped(self) -> Self {
        self.wrap(Wrap { trim: true })
    }
}
