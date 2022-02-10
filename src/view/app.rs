use crossterm::{
    event::{self, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{enable_raw_mode, EnterAlternateScreen},
};
use std::{
    error::Error,
    io,
    time::{Duration, Instant},
};
use tui::backend::{Backend, CrosstermBackend};
use tui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::Span,
    widgets::{
        canvas::{Canvas, Map, MapResolution, Rectangle},
        Block, Borders,
    },
    Frame, Terminal,
};

pub struct AppState {
    x: f64,
    y: f64,
    ball: Rectangle,
    playground: Rect,
    vx: f64,
    vy: f64,
    dir_x: bool,
    dir_y: bool,
}

impl Default for AppState {
    fn default() -> AppState {
        AppState {
            x: 0.0,
            y: 0.0,
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
    tick_rate: Duration,
    state: AppState,
}

impl App {
    pub fn new() -> Result<App, Box<dyn Error>> {
        // create app and run it
        let tick_rate = Duration::from_millis(250);
        Ok(App {
            tick_rate,
            state: AppState::default(),
        })
    }

    pub fn run(mut self) -> io::Result<()> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        let mut last_tick = Instant::now();
        loop {
            terminal.draw(|f| self.render(f))?;

            let timeout = self
                .tick_rate
                .checked_sub(last_tick.elapsed())
                .unwrap_or_else(|| Duration::from_secs(0));
            if event::poll(timeout)? {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Char('q') => {
                            return Ok(());
                        }
                        KeyCode::Down => {
                            self.state.y += 1.0;
                        }
                        KeyCode::Up => {
                            self.state.y -= 1.0;
                        }
                        KeyCode::Right => {
                            self.state.x += 1.0;
                        }
                        KeyCode::Left => {
                            self.state.x -= 1.0;
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
    fn update(&mut self) {
        if self.state.ball.x < self.state.playground.left() as f64
            || self.state.ball.x + self.state.ball.width > self.state.playground.right() as f64
        {
            self.state.dir_x = !self.state.dir_x;
        }
        if self.state.ball.y < self.state.playground.top() as f64
            || self.state.ball.y + self.state.ball.height > self.state.playground.bottom() as f64
        {
            self.state.dir_y = !self.state.dir_y;
        }

        if self.state.dir_x {
            self.state.ball.x += self.state.vx;
        } else {
            self.state.ball.x -= self.state.vx;
        }

        if self.state.dir_y {
            self.state.ball.y += self.state.vy;
        } else {
            self.state.ball.y -= self.state.vy
        }
    }

    fn render<B: Backend>(&self, f: &mut Frame<B>) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
            .split(f.size());
        let canvas = Canvas::default()
            .block(Block::default().borders(Borders::ALL).title("World"))
            .paint(|ctx| {
                ctx.draw(&Map {
                    color: Color::White,
                    resolution: MapResolution::High,
                });
                ctx.print(
                    self.state.x,
                    -self.state.y,
                    Span::styled("You are here", Style::default().fg(Color::Yellow)),
                );
            })
            .x_bounds([-180.0, 180.0])
            .y_bounds([-90.0, 90.0]);
        f.render_widget(canvas, chunks[0]);
        let canvas = Canvas::default()
            .block(Block::default().borders(Borders::ALL).title("Pong"))
            .paint(|ctx| {
                ctx.draw(&self.state.ball);
            })
            .x_bounds([10.0, 110.0])
            .y_bounds([10.0, 110.0]);
        f.render_widget(canvas, chunks[1]);
    }
}
