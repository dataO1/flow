use std::path::Path;

use indexmap::IndexSet;
use tui::{
    style::{Color, Style},
    text::Spans,
    widgets::{Block, Borders, List, ListItem, Widget},
};

use crate::view::model::track::Track;

pub struct TrackTableWidget<'a> {
    tracks: &'a TrackList,
    focused: bool,
}
impl<'a> TrackTableWidget<'a> {
    pub fn new(tracks: &'a TrackList, focused: bool) -> Self {
        Self { tracks, focused }
    }
}
impl<'a> Widget for TrackTableWidget<'a> {
    fn render(self, area: tui::layout::Rect, buf: &mut tui::buffer::Buffer) {
        let items: Vec<ListItem> = self
            .tracks
            .values()
            .into_iter()
            .map(|track| {
                let file_name = Path::new(&track.file_path)
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap();
                let spans = Spans::from(String::from(file_name));
                let item = ListItem::new(spans);
                let style = if let Some(focused) = self.tracks.get_focused() {
                    if *focused == *track {
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
        let list = List::new(items).block(Block::default().title("Files").borders(Borders::TOP));
        list.render(area, buf);
    }
}

//------------------------------------------------------------------//
//                            TrackList                             //
//------------------------------------------------------------------//
/// A struct for representing a list of tracks
pub struct TrackList {
    tracks: IndexSet<Track>,
    focused_track: Option<usize>,
    loaded_track: Option<usize>,
}

impl TrackList {
    /// returns a vector of tracks
    pub fn values(&self) -> &IndexSet<Track> {
        &self.tracks
    }

    pub fn sort() -> Self {
        todo!()
    }

    /// returns the currently focused track
    pub fn get_focused(&self) -> Option<&Track> {
        self.focused_track.map(|i| &self.tracks[i])
    }

    /// returns the currently loaded track
    pub fn get_loaded(&self) -> Option<&Track> {
        self.loaded_track.map(|i| &self.tracks[i])
    }

    /// focus next track and return it
    pub fn focus_next(&mut self) -> Option<&Track> {
        let new_index = self.focused_track.map(|i| {
            // check bounds
            if self.tracks.is_empty() {
                i
            } else {
                if i < self.tracks.len() - 1 {
                    i + 1
                } else {
                    // wrap list
                    0
                }
            }
        });
        // check bounds
        self.focused_track = new_index;
        self.get_focused()
    }

    /// focus previous track and return it
    pub fn focus_previous(&mut self) -> Option<&Track> {
        let new_index = self.focused_track.map(|i|
               // check bound 
               if i > 0 { i - 1 } else { 
                   // wrap list
                   self.tracks.len() - 1
               }
           );
        // check bounds
        self.focused_track = new_index;
        self.get_focused()
    }

    /// mark a track as loaded and return reference of loaded track
    pub fn load_focused(&mut self) -> Option<&Track> {
        self.loaded_track = self.focused_track;
        self.get_focused()
    }

    /// push a single track to the list
    pub fn insert(&mut self, track: Track) {
        if self.tracks.len() == 0 {
            self.focused_track = Some(0);
        }
        self.tracks.insert(track);
    }
}

impl<'a> Default for TrackList {
    fn default() -> Self {
        Self {
            tracks: IndexSet::default(),
            focused_track: None,
            loaded_track: None,
        }
    }
}
