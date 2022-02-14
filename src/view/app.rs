use crate::core::{
    analyzer::{self, Analyzer},
    player::{self},
};
use crossterm::{
    event::{self, EnableMouseCapture, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{enable_raw_mode, EnterAlternateScreen},
};
use std::{collections::HashMap, fs, io, path::Path, sync::Arc, time::Duration};
use tokio::{
    sync::mpsc::{channel, Receiver, Sender},
    task::JoinHandle,
};
use tui::{
    backend::{Backend, CrosstermBackend},
    text::Spans,
    widgets::{Block, Borders, List, ListItem, Paragraph},
};
use tui::{
    layout::{Constraint, Direction, Layout},
    Frame, Terminal,
};

use crate::core::player::{Message, Player};

use super::{
    model::track::Track,
    widgets::preview::{PreviewType, PreviewWidget},
};

#[derive(Clone, Debug)]
pub enum Event {
    /// Key event for Toggling playback
    TogglePlay,
    /// Key event for Loading the track under the cursor
    LoadTrack,
    /// Key event for quitting the application
    Quit,
    /// Unknown key event
    Unknown,
}
/// Abstraction layer for determining, which (key) events should get handled in which scope
#[derive(PartialEq)]
enum EventScope {
    Player,
    FileList,
}

pub struct App {
    //------------------------------------------------------------------//
    //                                UI                                //
    //------------------------------------------------------------------//
    /// text representation of latest event
    latest_event: String,
    /// Currently active component
    active_event_scope: EventScope,
    //------------------------------------------------------------------//
    //                              Player                              //
    //------------------------------------------------------------------//
    /// hashmap of tracks, that were found in the music dir
    tracks: HashMap<String, Track>,
    /// the track that is currently loaded by the player
    currently_loaded_track: Option<String>,
    /// current player position in number of packets.
    player_position: usize,
}

impl Default for App {
    fn default() -> Self {
        Self {
            player_position: 0,
            latest_event: String::from(""),
            tracks: HashMap::new(),
            currently_loaded_track: None,
            active_event_scope: EventScope::FileList,
        }
    }
}

impl App {
    /// start the app
    pub async fn run(mut self) -> io::Result<()> {
        // init terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        // create message passing channels
        let (player_events_out, mut player_events_in) = channel::<player::Event>(10);
        let (player_messages_out, player_messages_in) = channel::<player::Message>(10);
        let (analyzer_event_out, mut analyzer_event_in) = channel::<analyzer::Event>(10);
        // spawn player
        let player_handle = Player::spawn(player_messages_in, player_events_out);
        // list tracks TODO: read directory for files
        self.scan_dir(Path::new("/home/data01/Music/"));
        // spawn analyzers
        for track in &mut self.tracks.values() {
            Analyzer::spawn(
                track.file_path.to_owned(),
                Arc::clone(&track.preview_buffer),
                analyzer_event_out.clone(),
            );
        }
        loop {
            terminal.draw(|f| self.render(f))?;
            self.update(
                player_messages_out.clone(),
                &mut player_events_in,
                &mut analyzer_event_in,
            )
            .await;
        }
    }

    ///update the app's model
    async fn update(
        &mut self,
        player_messages_out: Sender<player::Message>,
        player_events_in: &mut Receiver<player::Event>,
        analyzer_event_in: &mut Receiver<analyzer::Event>,
    ) -> () {
        // get key events
        if let Ok(true) = event::poll(Duration::from_millis(1)) {
            if let event::Event::Key(key) = event::read().unwrap() {
                if let KeyModifiers::NONE = key.modifiers {
                    // Events with no modifiers (local)
                    match key.code {
                        /// Toggle Play
                        KeyCode::Char(' ') => {
                            player_messages_out.send(Message::TogglePlay).await;
                            self.latest_event = String::from("TogglePlay");
                        }
                        // Load Track
                        KeyCode::Enter => {
                            if self.active_event_scope != EventScope::FileList {
                                ()
                            };
                            // TODO: load track under cursor
                            if let Some(track) = self.tracks.values().next() {
                                player_messages_out
                                    .send(Message::Load(track.file_path.clone()))
                                    .await;
                                self.latest_event =
                                    String::from(format!("Loaded {}", track.file_path));
                                self.player_position = 0;
                                self.currently_loaded_track = Some(track.file_path.clone());
                            }
                        }
                        _ => self.latest_event = String::from("Unknown Command"),
                    }
                } else {
                    // Events with modifier (global)
                    match key {
                        KeyEvent {
                            code: KeyCode::Char('q'),
                            modifiers: KeyModifiers::ALT,
                        } => std::process::exit(0),
                        // unknown key command
                        _ => self.latest_event = String::from("Unknown Command"),
                    }
                };
            }
        }
        // get player events
        if let Ok(ev) = player_events_in.try_recv() {
            match ev {
                player::Event::PlayedPackages(num_packets) => {
                    self.player_position += num_packets;
                }
            }
        }
        // get analyzer events
        if let Ok(ev) = analyzer_event_in.try_recv() {
            match ev {
                analyzer::Event::DoneAnalyzing(track) => {
                    self.latest_event = String::from(format!("Analyzed: {}", track));
                }
            }
        }
    }

    /// define how the app should look like
    fn render<B: Backend>(&mut self, f: &mut Frame<B>) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    Constraint::Percentage(20),
                    Constraint::Percentage(10),
                    Constraint::Percentage(68),
                    Constraint::Percentage(2),
                ]
                .as_ref(),
            )
            .split(f.size());
        let hsplit = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(20), Constraint::Percentage(80)].as_ref())
            .split(chunks[2]);
        if let Some(path) = &self.currently_loaded_track {
            let curr_track = self.tracks.get(path).unwrap();
            let live_preview = PreviewWidget::new(
                PreviewType::LivePreview,
                Arc::clone(&curr_track.preview_buffer),
                self.player_position,
            );
            let preview = PreviewWidget::new(
                PreviewType::Preview,
                Arc::clone(&curr_track.preview_buffer),
                self.player_position,
            );

            // f.render_widget(preview, chunks[1]);
            f.render_widget(live_preview, chunks[0]);
        }

        let status_bar = Paragraph::new(self.latest_event.clone())
            .block(
                Block::default()
                    // .title("Status")
                    .title_alignment(tui::layout::Alignment::Center)
                    .borders(Borders::TOP),
            )
            .alignment(tui::layout::Alignment::Center);
        f.render_widget(status_bar, chunks[3]);
        let tracks: Vec<ListItem> = self
            .tracks
            .keys()
            .cloned()
            .map(|file_path| {
                // parse file path to file name
                let file_name = Path::new(&file_path).file_name().unwrap().to_str().unwrap();
                ListItem::new(Spans::from(String::from(file_name)))
            })
            .collect();
        let track_list = List::new(tracks).block(
            Block::default()
                .title("Files")
                .borders(Borders::TOP | Borders::RIGHT),
        );
        f.render_widget(track_list, hsplit[0]);
    }

    /// scans a directory for tracks
    /// Supported file types are .mp3 .flac .wav
    fn scan_dir(&mut self, dir: &Path) -> io::Result<()> {
        if dir.is_dir() {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    self.scan_dir(&path)?;
                } else {
                    //TODO: use path object for hashmap
                    let extension = path.extension().unwrap().to_str().unwrap();
                    let supported_extensions = ["mp3", "wav", "flac"];
                    if supported_extensions.contains(&extension) {
                        let file_path = entry.path().into_os_string().into_string().unwrap();
                        // let file_name = entry.file_name().into_string().unwrap();
                        let track = Track::new(String::from(file_path.clone()));
                        self.tracks.insert(file_path, track);
                        // self.tracks.insert(file_name, track);
                    };
                }
            }
        };
        Ok(())
    }
}
