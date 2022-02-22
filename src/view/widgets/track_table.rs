
use std::sync::Arc;

use indexmap::IndexSet;
use tui::{layout::Constraint, style::{Color, Modifier, Style}, widgets::{Block, Borders, Cell, Row, Table, Widget}};

use crate::view::model::track::Track;

//------------------------------------------------------------------//
//                         TrackTableWidget                         //
//------------------------------------------------------------------//

/// A Widget for visualizing a TrackList in table form
pub struct TrackTableWidget<'a> {
    tracks: &'a TrackList,
    focused: bool,
}
impl<'a> TrackTableWidget<'a> {
    pub fn new(tracks: &'a TrackList, focused: bool) -> Self {
        Self { tracks, focused }
    }

    /// returns a TUI Row objed, with specific styling based on, whether the row is focused or an
    /// alternating row (every other row)
    fn get_row(&self, track:&Track, focused: bool)-> Row{
        // || filename || analyzed_percentage
        //
        // if progress could be computed return it in formatted form, else return string "NaN"
        let progress_string = track.progress().map_or(String::from("Nan"),|progress|{ format!("{}%", progress) });
        let bpm = format!("{}",track.meta.read().unwrap().bpm);
        let style = if focused {Style::default().fg(Color::Black).bg(Color::DarkGray)}else {Style::default()};
        Row::new(vec![Cell::from(track.file_name.to_string())
                 , Cell::from(progress_string), Cell::from(bpm)]).style(style)
    }

    fn get_header(&self) -> Row {
        // || filename || analyzed_percentage
        let style = Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
        Row::new(vec!["File Name", "Analysis", "BPM"]).bottom_margin(0).style(style).bottom_margin(1)
    }
}
impl<'a> Widget for TrackTableWidget<'a> {
    fn render(self, area: tui::layout::Rect, buf: &mut tui::buffer::Buffer) {
        let header = self.get_header();
        let num_colums = 3 as usize;
        let auto_widths = vec![Constraint::Percentage(100/num_colums as u16);num_colums];
        let rows: Vec<Row> = self
            .tracks
            .values()
            .into_iter()
            .map(|track| {
                let focused = self.tracks.get_focused().map(|f| f == *track).unwrap_or(false);
                self.get_row(&track, focused)
            })
            .collect();
        let table = Table::new(rows)
            .block(Block::default().title("Files").borders(Borders::TOP)).header(header).style(Style::default().fg(Color::White)).widths(&auto_widths).column_spacing(1);
        table.render(area, buf);
    }
}

//------------------------------------------------------------------//
//                            TrackList                             //
//------------------------------------------------------------------//

/// A struct for representing a list of tracks
pub struct TrackList {
    tracks: IndexSet<Arc<Track>>,
    focused_track: Option<usize>,
    loaded_track: Option<usize>,
}

impl TrackList {
    /// returns a vector of tracks
    pub fn values(&self) -> &IndexSet<Arc<Track>> {
        &self.tracks
    }

    // pub fn sort(&mut self) {
    //     self.tracks.sort();
    // }

    pub fn sort_by(&mut self){
        todo!();
    }

    /// returns the currently focused track
    pub fn get_focused(&self) -> Option<Arc<Track>> {
        self.focused_track.map(|i| { 
            let track = &self.tracks[i];
            Arc::clone(track) })
    }

    /// returns the currently loaded track
    pub fn get_loaded(&self) -> Option<Arc<Track>> {
        self.loaded_track.map(|i| { 
            let track = &self.tracks[i];
            Arc::clone(track) })
    }

    /// focus next track and return it
    pub fn focus_next(&mut self) -> Option<Arc<Track>> {
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
    pub fn focus_previous(&mut self) -> Option<Arc<Track>> {
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
    pub fn load_focused(&mut self) -> Option<Arc<Track>> {
        self.loaded_track = self.focused_track;
        self.get_focused()
    }

    /// push a single track to the list
    pub fn insert(&mut self, track: Arc<Track>) {
        if self.tracks.len() == 0 {
            self.focused_track = Some(0);
        }
        self.tracks.insert(Arc::clone(&track));
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
