use tui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    widgets::{Clear, Widget},
};

pub struct PopupWidget<T: Widget> {
    percent_x: u16,
    percent_y: u16,
    widget: T,
}
impl<T: Widget> PopupWidget<T> {
    pub fn new(widget: T, percent_x: u16, percent_y: u16) -> Self {
        Self {
            percent_x,
            percent_y,
            widget,
        }
    }
    /// Helper function to create a centered rect using up a percentage of the given parent
    /// rect
    fn centered_rect(&self, percent_x: u16, percent_y: u16, r: Rect) -> Rect {
        let popup_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    Constraint::Percentage((100 - percent_y) / 2),
                    Constraint::Percentage(percent_y),
                    Constraint::Percentage((100 - percent_y) / 2),
                ]
                .as_ref(),
            )
            .split(r);
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints(
                [
                    Constraint::Percentage((100 - percent_x) / 2),
                    Constraint::Percentage(percent_x),
                    Constraint::Percentage((100 - percent_x) / 2),
                ]
                .as_ref(),
            )
            .split(popup_layout[1])[1]
    }
}

impl<T: Widget> Widget for PopupWidget<T> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let popup_area = self.centered_rect(self.percent_x, self.percent_y, area);
        // clear background of target area
        Clear.render(popup_area, buf);
        // draw the child widget on popup area
        self.widget.render(popup_area, buf);
    }
}
