use crate::core::{
    analyzer::{self, Analyzer},
    player::{self, TimeMarker},
};
use crossterm::{
    event::{self, EnableMouseCapture, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{enable_raw_mode, EnterAlternateScreen},
};
use symphonia::core::units::Time;

use std::sync::mpsc::{channel, Receiver, Sender};
use std::{
    fs, io,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};
use tui::{
    backend::{Backend, CrosstermBackend},
    widgets::{Block, Borders, Paragraph},
};
use tui::{
    layout::{Constraint, Direction, Layout},
    Frame, Terminal,
};

use crate::core::player::{Message, Player};

use super::widgets::{
    live_preview::LivePreviewWidget,
    preview::PreviewWidget,
    track_table::{TrackList, TrackTableWidget},
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
    tracks: TrackList,
    /// current player position in number of packets.
    player_position: Arc<Mutex<Option<TimeMarker>>>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            player_position: Arc::new(Mutex::new(None)),
            latest_event: String::from(""),
            tracks: TrackList::default(),
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
        let (player_events_out, mut player_events_in) = channel::<player::Event>();
        let (player_messages_out, player_messages_in) = channel::<player::Message>();
        let (analyzer_event_out, mut analyzer_event_in) = channel::<analyzer::Event>();
        // spawn player
        let player_handle = Player::spawn(
            Arc::clone(&self.player_position),
            player_messages_in,
            player_events_out,
        );
        // list tracks TODO: read directory for files
        let files = self.scan_dir(Path::new("/home/data01/Music/")).unwrap();
        // spawn analyzers
        for file in files {
            Analyzer::spawn(file, analyzer_event_out.clone());
        }
        loop {
            terminal.draw(|f| self.render(f))?;
            // only take key events every 250 milliseconds
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
        //------------------------------------------------------------------//
        //                            Key Events                            //
        //------------------------------------------------------------------//
        if let Ok(true) = event::poll(Duration::from_micros(1)) {
            if let event::Event::Key(key) = event::read().unwrap() {
                if let KeyModifiers::NONE = key.modifiers {
                    // Events with no modifiers (local)
                    match key.code {
                        // go up a track
                        KeyCode::Char('j') => {
                            self.tracks.focus_next();
                        }
                        // go down a track
                        KeyCode::Char('k') => {
                            self.tracks.focus_previous();
                        }
                        // skip backwards
                        KeyCode::Char('h') => {
                            player_messages_out
                                .send(Message::SkipBackward(Time::new(20, 0.)))
                                .unwrap();
                        }
                        // skip forward
                        KeyCode::Char('l') => player_messages_out
                            .send(Message::SkipForward(Time::new(20, 0.)))
                            .unwrap(),
                        // Toggle Play
                        KeyCode::Char(' ') => {
                            player_messages_out.send(Message::TogglePlay).unwrap();
                            self.latest_event = String::from("TogglePlay");
                        }
                        KeyCode::Char('c') => player_messages_out.send(Message::Cue).unwrap(),
                        // Load Track
                        KeyCode::Enter => {
                            if self.active_event_scope != EventScope::FileList {
                                ()
                            };
                            let focused = self.tracks.load_focused();
                            if let Some(track) = focused {
                                player_messages_out
                                    .send(Message::Load(track.file_path.clone()))
                                    .unwrap();
                                self.latest_event =
                                    String::from(format!("Loaded {}", track.file_path));
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
        //------------------------------------------------------------------//
        //                          Player Events                           //
        //------------------------------------------------------------------//
        // if let Ok(ev) = player_events_in.try_recv() {
        //     match ev {
        //         player::Event::PlayedPackages(num_packets) => {
        //             self.player_position += num_packets;
        //         }
        //     }
        // }
        //------------------------------------------------------------------//
        //                         Analyzer Events                          //
        //------------------------------------------------------------------//
        if let Ok(ev) = analyzer_event_in.try_recv() {
            match ev {
                analyzer::Event::DoneAnalyzing(track) => {
                    self.latest_event = String::from(format!("Analyzed: {}", track));
                }
                analyzer::Event::NewTrack(track) => self.tracks.insert(track),
            }
        }
    }

    /// define how the app should look like
    fn render<B: Backend>(&mut self, f: &mut Frame<B>) {
        // TODO: refactor
        let window = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    // split for the live preview
                    Constraint::Percentage(20),
                    // split for the waveform overview
                    Constraint::Percentage(5),
                    // split for the main body
                    Constraint::Percentage(73),
                    // split for the footer
                    Constraint::Percentage(2),
                ]
                .as_ref(),
            )
            .split(f.size());
        let player_position = (*self.player_position.lock().unwrap()).clone();
        if let Some(track) = self.tracks.get_loaded() {
            let live_preview = LivePreviewWidget::new(&track, &player_position);
            let preview = PreviewWidget::new(&track, 0);

            // f.render_widget(preview, window[1]);
            f.render_widget(live_preview, window[0]);
        }

        let status_bar = Paragraph::new(self.latest_event.clone())
            .block(
                Block::default()
                    // .title("Status")
                    .title_alignment(tui::layout::Alignment::Center)
                    .borders(Borders::TOP),
            )
            .alignment(tui::layout::Alignment::Center);
        f.render_widget(status_bar, window[3]);
        let track_table = TrackTableWidget::new(
            &self.tracks,
            self.active_event_scope == EventScope::FileList,
        );
        f.render_widget(track_table, window[2]);
        // let block = Block::default().title("popup").borders(Borders::ALL);
        // let popup = PopupWidget::new(block, 10, 90);
        // f.render_widget(popup, f.size());
    }

    /// scans a directory for tracks
    /// Supported file types are .mp3 .flac .wav
    fn scan_dir(&mut self, dir: &Path) -> io::Result<Vec<String>> {
        let mut res = vec![];
        if dir.is_dir() {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    let mut sub_dirs = self.scan_dir(&path)?;
                    res.append(&mut sub_dirs);
                } else {
                    //TODO: use path object for hashmap
                    let extension = path.extension().unwrap().to_str().unwrap();
                    let supported_extensions = ["mp3", "wav", "flac"];
                    if supported_extensions.contains(&extension) {
                        let file_path = entry.path().into_os_string().into_string().unwrap();
                        // let file_name = entry.file_name().into_string().unwrap();
                        res.push(file_path);
                    };
                }
            }
        };
        Ok(res)
    }
}
