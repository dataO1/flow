use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tui::buffer::Buffer;
use tui::layout::Rect;
use tui::style::Color;
use tui::widgets::canvas::{Canvas, Line};
use tui::widgets::{Block, Borders, Widget};

use crate::core::player::FrameBuffer;

pub struct WaveWidget {
    preview_buf: Arc<Mutex<FrameBuffer>>,
}

impl WaveWidget {
    pub fn new(preview_buf: Arc<Mutex<FrameBuffer>>) -> Self {
        Self { preview_buf }
    }

    /// tries to detect transients and gives them color
    fn get_col(&self, prev: &Sample, curr: &Sample) -> Color {
        let diff = curr - prev;
        // try to detect transient
        if diff > 0.6 {
            Color::Red
        } else {
            Color::Green
        }
    }
}

impl Widget for WaveWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // this determines how many samples are "chunked" and thus displayed together as one line,
        // to fit the resolution of the given area
        let x_max = (area.width as f64 / 2.0).floor() as i16;
        let x_min = -x_max;
        let y_max = (area.height as f64 / 2.0).floor() as i16;
        let y_min = -y_max;
        let preview_buf = self.preview_buf.lock().unwrap();
        // println!("x:({},{}), y:({}{})", x_min, x_max, y_min, y_max);
        // println!("preview_buf_len: {}", preview_buf.len());
        let can = Canvas::default()
            .block(Block::default().title("Live Preview").borders(Borders::ALL))
            .x_bounds([x_min as f64, x_max as f64])
            .y_bounds([y_min as f64, y_max as f64])
            .paint(|ctx| {
                let mut prev = 0.0 as f32;
                // center line
                ctx.draw(&Line {
                    x1: 0.0,
                    x2: 0.0,
                    y1: y_min as f64,
                    y2: y_max as f64,
                    color: Color::Red,
                });
                ctx.layer();
                // for i in (1..(area.width as usize)) {
                for (i, sample) in preview_buf
                    .get_preview(area.width as usize)
                    .into_iter()
                    .take(area.width as usize)
                    .enumerate()
                {
                    // determine x
                    // let x = ((i * chunk_size) as f32) - (preview_buf_len as f32 / 2.0);
                    // fit sample (a value between 0 and 1) into area height
                    let x = x_min + i as i16;
                    let y = sample * (area.height as f32);
                    let y = 5. * y;
                    // draw main line
                    ctx.draw(&Line {
                        x1: x as f64,
                        x2: x as f64,
                        y1: y as f64,
                        y2: -y as f64,
                        color: Color::Gray,
                    });
                    // draw main line
                    ctx.draw(&Line {
                        x1: x as f64,
                        x2: x as f64,
                        y1: y as f64 * 0.5,
                        y2: -y as f64 * 0.5,
                        color: self.get_col(&sample, &prev),
                    });
                    prev = sample;
                }
            });
        can.render(area, buf);
    }
}

pub type Sample = f32;

/// A buffer to hold audio data meant for display on the terminal.
#[derive(Debug, PartialEq)]
pub struct DataBuffer {
    buffer: VecDeque<Sample>,
}

impl DataBuffer {
    /// Makes a zero-filled circular data buffer of the given size.
    pub fn new(len: usize) -> DataBuffer {
        DataBuffer {
            buffer: VecDeque::from(vec![0.; len]),
        }
    }

    /// Adds the data to the queue.
    ///
    /// The latest data from `buf_data` is pushed to the end of the DataBuffer. If buf_data is
    /// larger than the DataBuffer, only available samples will be used. if buf_data is smaller,
    /// the remaining space is filled with the previous most recent.
    pub fn push_latest_data(&mut self, buf_data: Vec<Sample>) {
        if buf_data.len() < self.buffer.len() {
            let diff = self.buffer.len() - buf_data.len();

            // Shift the preserved end data to the beginning
            for index in 0..diff {
                self.buffer[index] = self.buffer[index + buf_data.len()];
            }

            // fill the remaining data from the buf_data
            for (index, item) in buf_data.iter().enumerate() {
                self.buffer[index + diff] = *item;
            }
        } else {
            let diff = buf_data.len() - self.buffer.len();

            // Fill the latest available data that will fit.

            // TODO: Complicatedness below avoids a for loop lint. Nice experiment, but maybe find
            // a better way to solve?
            let (left, right) = self.buffer.as_mut_slices();
            let buf_data_source = &buf_data[diff..];
            left.copy_from_slice(&buf_data_source[..left.len()]);
            right.copy_from_slice(&buf_data_source[left.len()..]);
        }
    }

    /// Returns the length of the buffer
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Returns an iter from the underlying VecDeque
    pub fn iter(&self) -> std::collections::vec_deque::Iter<Sample> {
        self.buffer.iter()
    }
}
