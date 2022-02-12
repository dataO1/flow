use crate::view::widgets::wave::WaveWidget;
use crate::Event;
use crossterm::{
    event::{self, EnableMouseCapture, KeyCode},
    execute,
    terminal::{enable_raw_mode, EnterAlternateScreen},
};
use rand::Rng;
use std::{
    io,
    sync::{Arc, Mutex},
};
use std::{thread, time};
use tokio::{
    sync::mpsc::{channel, Receiver, Sender},
    task::JoinHandle,
};
use tui::backend::{Backend, CrosstermBackend};
use tui::{
    layout::{Constraint, Direction, Layout},
    widgets::{Block, Borders},
    Frame, Terminal,
};

use crate::core::player::{Player, PlayerMessage};

use super::widgets::wave::DataBuffer;

const MAX_BUFFER_SAMPLES: usize = 1000;

/// Represents the App's State
pub struct AppState {}

impl Default for AppState {
    fn default() -> AppState {
        AppState {}
    }
}

pub struct App {
    /// a sender channel to the Player thread
    player_handle: Sender<PlayerMessage>,
    /// shared audio buffer
    audio_buffer: DataBuffer,
    /// the receiver end of Events
    event_channel_rx: Receiver<Event>,
    /// the transmitter end of Events
    event_channel_tx: Sender<Event>,
}

impl App {
    pub fn new() -> App {
        // create app and run it
        let (tx, rx) = channel::<Event>(1);
        App {
            player_handle: Player::spawn(tx.clone()),
            audio_buffer: DataBuffer::new(MAX_BUFFER_SAMPLES),
            event_channel_rx: rx,
            event_channel_tx: tx.clone(),
        }
    }

    pub async fn run(mut self) -> io::Result<()> {
        // init terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        // App::simulate_filling_audio_buffer(Arc::clone(&self.audio_buffer)); // this is just for testing
        // spawn the input thread
        let _kb_join_handle = App::spawn_key_handler(self.event_channel_tx.clone());
        // execute main UI loop
        loop {
            // draw to terminal
            terminal.draw(|f| self.layout(f))?;
            // // get events async
            // if let Some(ev) = self.event_channel_rx.recv().await {
            //     // update state
            //     self.update(ev).await;
            // }
            // get events async
            if let Ok(ev) = self.event_channel_rx.try_recv() {
                // update state
                self.update(ev).await;
            }
        }
    }

    fn spawn_key_handler(app: Sender<Event>) -> JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                if let crossterm::event::Event::Key(key) = event::read().unwrap() {
                    let ev = match key.code {
                        KeyCode::Enter => Event::LoadTrack(String::from("music/bass_symptom.mp3")),
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
    async fn update(&mut self, ev: Event) {
        match ev {
            Event::TogglePlay => {
                self.player_handle.send(PlayerMessage::TogglePlay).await;
            }
            Event::LoadTrack(track) => {
                self.player_handle.send(PlayerMessage::Load(track)).await;
            }
            Event::SamplePlayed(samples) => {
                self.audio_buffer.push_latest_data(samples);
            }
            Event::Quit => std::process::exit(0),
            Event::Unknown => todo!(),
        }
    }

    /// define how the app should look like
    fn layout<B: Backend>(&self, f: &mut Frame<B>) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(30), Constraint::Percentage(70)].as_ref())
            .split(f.size());
        let wave_widget = WaveWidget::new(&self.audio_buffer);
        f.render_widget(wave_widget, chunks[0]);
    }
}
