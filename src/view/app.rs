use crate::core::player::{self, PreviewBuffer};
use crossterm::{
    event::{self, EnableMouseCapture, KeyCode},
    execute,
    terminal::{enable_raw_mode, EnterAlternateScreen},
};
use std::{
    io,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{
    sync::mpsc::{channel, Receiver, Sender},
    task::JoinHandle,
};
use tui::{
    backend::{Backend, CrosstermBackend},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use tui::{
    layout::{Constraint, Direction, Layout},
    Frame, Terminal,
};

use crate::core::player::{Message, Player};

use super::widgets::preview::{PreviewType, PreviewWidget};

#[derive(Clone, Debug)]
pub enum Event {
    TogglePlay,
    LoadTrack(String),
    Quit,
    Unknown,
}

pub struct App {
    frame_buf: Arc<Mutex<PreviewBuffer>>,
    player_position: usize,
    status_text: String,
}

impl Default for App {
    fn default() -> Self {
        Self {
            frame_buf: Arc::new(Mutex::new(PreviewBuffer::default())),
            player_position: 0,
            status_text: String::from(""),
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
        // create all message passing channels
        let (key_events_out, mut key_events_in) = channel::<Event>(10);
        let (player_events_out, mut player_events_in) = channel::<player::Event>(10);
        let (player_messages_out, player_messages_in) = channel::<player::Message>(10);
        // spawn the input thread
        let _kb_join_handle = App::spawn_key_handler(key_events_out.clone());
        let player_handle = Player::spawn(
            player_messages_in,
            player_events_out,
            Arc::clone(&self.frame_buf),
        );
        // let tick_rate = Duration::from_millis(5);
        // let mut last_tick = Instant::now();
        // execute main UI loop
        loop {
            // if last_tick.elapsed() >= tick_rate {
            // draw to terminal
            terminal.draw(|f| self.layout(f))?;
            //     last_tick = Instant::now();
            // }
            // // get events async
            // if let Some(ev) = self.event_channel_rx.recv().await {
            //     // update state
            //     self.update(ev).await;
            // }
            // get events async
            // update state
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
                        KeyCode::Enter => {
                            Event::LoadTrack(String::from("/home/data01/Downloads/the_rush.mp3"))
                        }
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
                Event::LoadTrack(track) => {
                    player_messages_out.send(Message::Load(track)).await;
                    self.status_text = String::from("Loaded track")
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
                _ => {}
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
        let live_preview = PreviewWidget::new(
            PreviewType::LivePreview,
            Arc::clone(&self.frame_buf),
            self.player_position,
        );
        let preview = PreviewWidget::new(
            PreviewType::Preview,
            Arc::clone(&self.frame_buf),
            self.player_position,
        );

        f.render_widget(preview, chunks[1]);
        f.render_widget(live_preview, chunks[0]);

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
}
