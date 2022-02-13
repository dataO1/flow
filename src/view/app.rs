use crate::core::{
    analyzer::Analyzer,
    player::{self, PreviewBuffer},
};
use crossterm::{
    event::{self, EnableMouseCapture, KeyCode},
    execute,
    terminal::{enable_raw_mode, EnterAlternateScreen},
};
use std::{
    collections::HashMap,
    fs, io,
    path::Path,
    sync::{Arc, Mutex},
};
use tokio::{
    sync::mpsc::{channel, Receiver, Sender},
    task::JoinHandle,
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

use super::{
    model::track::Track,
    widgets::preview::{PreviewType, PreviewWidget},
};

#[derive(Clone, Debug)]
pub enum Event {
    TogglePlay,
    LoadTrack,
    Quit,
    Unknown,
}

pub struct App {
    frame_buf: Arc<Mutex<PreviewBuffer>>,
    player_position: usize,
    status_text: String,
    tracks: HashMap<String, Track>,
    currently_loaded_track: Option<String>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            frame_buf: Arc::new(Mutex::new(PreviewBuffer::default())),
            player_position: 0,
            status_text: String::from(""),
            tracks: HashMap::new(),
            currently_loaded_track: None,
        }
    }
}

impl App {
    pub async fn run(mut self) -> io::Result<()> {
        // init terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        // create message passing channels
        let (key_events_out, mut key_events_in) = channel::<Event>(10);
        let (player_events_out, mut player_events_in) = channel::<player::Event>(10);
        let (player_messages_out, player_messages_in) = channel::<player::Message>(10);
        // spawn the input thread
        let _kb_join_handle = App::spawn_key_handler(key_events_out.clone());
        // spawn player
        let player_handle = Player::spawn(player_messages_in, player_events_out);
        // list tracks TODO: read directory for files
        self.scan_dir(Path::new("/home/data01/Music/"));
        // spawn analyzers
        for track in &mut self.tracks.values() {
            Analyzer::spawn(
                track.file_path.to_owned(),
                Arc::clone(&track.preview_buffer),
            );
        }
        loop {
            terminal.draw(|f| self.layout(f))?;
            self.update(
                &mut key_events_in,
                player_messages_out.clone(),
                &mut player_events_in,
            )
            .await;
        }
    }

    fn spawn_key_handler(app: Sender<Event>) -> JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                if let crossterm::event::Event::Key(key) = event::read().unwrap() {
                    let ev = match key.code {
                        KeyCode::Enter => Event::LoadTrack,
                        KeyCode::Char(' ') => Event::TogglePlay,
                        KeyCode::Char('q') => Event::Quit,
                        _ => Event::Unknown,
                    };
                    match app.send(ev).await {
                        Ok(_res) => (),
                        Err(err) => {
                            println!("Error:{:#?}", err)
                        }
                    }
                };
            }
        })
    }

    ///update the app's model
    async fn update(
        &mut self,
        key_events_in: &mut Receiver<Event>,
        player_messages_out: Sender<player::Message>,
        player_events_in: &mut Receiver<player::Event>,
    ) -> () {
        if let Ok(ev) = key_events_in.try_recv() {
            match ev {
                Event::TogglePlay => {
                    player_messages_out.send(Message::TogglePlay).await;
                    self.status_text = String::from("TogglePlay");
                }
                Event::LoadTrack => {
                    // TODO: actually load track under cursor
                    if let Some(track) = self.tracks.values().next() {
                        player_messages_out
                            .send(Message::Load(track.file_path.clone()))
                            .await;
                        self.status_text = String::from("Loaded track");
                        self.player_position = 0;
                        self.currently_loaded_track = Some(track.file_path.clone());
                    }
                }
                Event::Quit => std::process::exit(0),
                Event::Unknown => {
                    //ignore unknown commands
                }
            }
        };
        if let Ok(ev) = player_events_in.try_recv() {
            match ev {
                player::Event::PlayedPackage(num_packets) => {
                    self.player_position += num_packets;
                }
            }
        }
    }

    /// define how the app should look like
    fn layout<B: Backend>(&mut self, f: &mut Frame<B>) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    Constraint::Percentage(20),
                    Constraint::Percentage(10),
                    Constraint::Percentage(63),
                    Constraint::Percentage(2),
                ]
                .as_ref(),
            )
            .split(f.size());
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

            f.render_widget(preview, chunks[1]);
            f.render_widget(live_preview, chunks[0]);
        }

        let status_bar = Paragraph::new(self.status_text.clone())
            .block(
                Block::default()
                    // .title("Status")
                    .title_alignment(tui::layout::Alignment::Center)
                    .borders(Borders::TOP),
            )
            .alignment(tui::layout::Alignment::Center);
        f.render_widget(status_bar, chunks[3]);
    }

    fn scan_dir(&mut self, dir: &Path) -> io::Result<()> {
        if dir.is_dir() {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    self.scan_dir(&path)?;
                } else {
                    let file_path = path.into_os_string().into_string().unwrap();
                    let track = Track::new(String::from(file_path.clone()));
                    self.tracks.insert(file_path, track);
                }
            }
        };
        Ok(())
    }
}
