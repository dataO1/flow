use crate::view::widgets::wave::WaveWidget;
use crossterm::{
    event::{self, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{enable_raw_mode, EnterAlternateScreen},
};
use rand::Rng;
use std::{
    io,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use std::{thread, time};
use tokio::sync::mpsc::Sender;
use tui::backend::{Backend, CrosstermBackend};
use tui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Color,
    widgets::{canvas::Rectangle, Block, Borders},
    Frame, Terminal,
};

use crate::core::player::{Command, Message, Player};

use super::widgets::wave::DataBuffer;

const MAX_BUFFER_SAMPLES: usize = 1000;

/// Represents the App's State
pub struct AppState {
    ball: Rectangle,
    playground: Rect,
    vx: f64,
    vy: f64,
    dir_x: bool,
    dir_y: bool,
}

impl AppState {
    /// update the app's state
    fn update(&mut self) {
        if self.ball.x < self.playground.left() as f64
            || self.ball.x + self.ball.width > self.playground.right() as f64
        {
            self.dir_x = !self.dir_x;
        }
        if self.ball.y < self.playground.top() as f64
            || self.ball.y + self.ball.height > self.playground.bottom() as f64
        {
            self.dir_y = !self.dir_y;
        }

        if self.dir_x {
            self.ball.x += self.vx;
        } else {
            self.ball.x -= self.vx;
        }

        if self.dir_y {
            self.ball.y += self.vy;
        } else {
            self.ball.y -= self.vy
        }
    }
}

impl Default for AppState {
    fn default() -> AppState {
        AppState {
            ball: Rectangle {
                x: 10.0,
                y: 30.0,
                width: 10.0,
                height: 10.0,
                color: Color::Yellow,
            },
            playground: Rect::new(10, 10, 100, 100),
            vx: 1.0,
            vy: 1.0,
            dir_x: true,
            dir_y: true,
        }
    }
}

pub struct App {
    /// update rate of the app (i.e. every 25 ms)
    tick_rate: Duration,
    /// the apps internal state
    state: AppState,
    /// all loaded widgets, the app needs
    // widgets: Vec<Box<dyn Widget>>,
    /// a sender channel to the Player thread
    player_handle: Sender<Message>,
    audio_buffer: Arc<Mutex<DataBuffer>>,
}

impl App {
    pub fn new() -> App {
        // create app and run it
        let tick_rate = Duration::from_millis(250);
        App {
            tick_rate,
            state: AppState::default(),
            // widgets: vec![],
            player_handle: Player::spawn(),
            audio_buffer: Arc::new(Mutex::new(DataBuffer::new(MAX_BUFFER_SAMPLES))),
        }
    }

    fn simulate_filling_audio_buffer(buf: Arc<Mutex<DataBuffer>>) {
        tokio::spawn(async move {
            let mut rng = rand::thread_rng();
            let mut r = Player::new_reader("music/bass_symptom.mp3");
            let mut c = 0;
            while let Ok(p) = r.next_packet() {
                for smp in p.buf().into_iter() {
                    thread::sleep(time::Duration::from_millis(100));
                    buf.lock().unwrap().push_latest_data(&[rng.gen()]);
                    // buf.lock().unwrap().push_latest_data(&[smp.clone() as f32]);
                    c += 1;
                }
                c = 0;
            }
        });
    }
    /// Run the application. Handles Keyboard input and the rendering of the app.
    pub async fn run(mut self) -> io::Result<()> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        let mut last_tick = Instant::now();
        App::simulate_filling_audio_buffer(Arc::clone(&self.audio_buffer));
        loop {
            terminal.draw(|f| self.layout(f))?;

            // TODO: what is this?
            let timeout = self
                .tick_rate
                .checked_sub(last_tick.elapsed())
                .unwrap_or_else(|| Duration::from_secs(0));
            // TODO: Error handling
            if event::poll(timeout)? {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Enter => {
                            self.player_handle
                                .send(Message::Command(Command::Load(String::from(
                                    "music/bass_symptom.mp3",
                                ))))
                                .await
                                .unwrap();
                        }
                        KeyCode::Char(' ') => {
                            self.player_handle
                                .send(Message::Command(Command::TogglePlay))
                                .await
                                .unwrap();
                        }
                        KeyCode::Char('q') => {
                            self.player_handle
                                .send(Message::Command(Command::Close))
                                .await
                                .unwrap();
                            return Ok(());
                        }
                        _ => {}
                    }
                }
            }

            if last_tick.elapsed() >= self.tick_rate {
                self.update();
                last_tick = Instant::now();
            }
        }
    }

    ///update the app's model
    fn update(&mut self) {
        self.state.update()
    }

    /// define how the app should look like
    fn layout<B: Backend>(&self, f: &mut Frame<B>) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(3)].as_ref())
            .split(f.size());
        let waveform_block = Block::default()
            .borders(Borders::ALL)
            .title("Waveform Oscilloscope");
        let buffer = self.audio_buffer.lock().unwrap();
        let wave_widget = WaveWidget::new(&buffer);
        f.render_widget(wave_widget, waveform_block.inner(chunks[0]));
    }
}
