use std::path::Path;

use tui::{
    style::{Color, Style},
    text::Spans,
    widgets::{Block, Borders, List, ListItem, Widget},
};

pub struct FileListWidget<'a> {
    files: &'a Vec<String>,
    focused: bool,
    focused_track: &'a Option<String>,
}
impl<'a> FileListWidget<'a> {
    pub fn new(tracks: &'a Vec<String>, focused: bool, focused_track: &'a Option<String>) -> Self {
        Self {
            files: tracks,
            focused,
            focused_track,
        }
    }
}
impl<'a> Widget for FileListWidget<'a> {
    fn render(self, area: tui::layout::Rect, buf: &mut tui::buffer::Buffer) {
        let items: Vec<ListItem> = self
            .files
            .into_iter()
            .map(|file_path| {
                let file_name = Path::new(&file_path).file_name().unwrap().to_str().unwrap();
                let spans = Spans::from(String::from(file_name));
                let mut item = ListItem::new(spans);
                let style = if let Some(file) = self.focused_track {
                    if file == file_path {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default()
                    }
                } else {
                    Style::default()
                };
                item.style(style)
            })
            .collect();
        let list = List::new(items).block(
            Block::default()
                .title("Files")
                .borders(Borders::TOP | Borders::RIGHT),
        );
        list.render(area, buf);
    }
}
