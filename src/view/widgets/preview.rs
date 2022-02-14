use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tui::buffer::Buffer;
use tui::layout::Rect;
use tui::style::Color;
use tui::widgets::canvas::{Canvas, Line};
use tui::widgets::{Block, Borders, Widget};

use crate::core::player::PreviewBuffer;

pub struct PreviewWidget {
    preview_buf: Arc<Mutex<PreviewBuffer>>,
    player_pos: usize,
    preview_type: PreviewType,
}

#[derive(Copy, Clone, PartialEq)]
pub enum PreviewType {
    LivePreview,
    Preview,
}

impl PreviewWidget {
    pub fn new(
        preview_type: PreviewType,
        preview_buf: Arc<Mutex<PreviewBuffer>>,
        player_pos: usize,
    ) -> Self {
        Self {
            preview_buf,
            player_pos,
            preview_type,
        }
    }

    /// tries to detect transients and gives them color
    fn get_col(&self, prev: &Sample, curr: &Sample) -> Color {
        let diff = curr - prev;
        // try to detect transient
        if diff > 0.1 {
            Color::Red
        } else {
            Color::Green
        }
    }
}

impl Widget for PreviewWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // this determines how many samples are "chunked" and thus displayed together as one line,
        // to fit the resolution of the given area
        let x_max = area.width as usize;
        let y_max = area.height as usize;
        let playhead_offset_from_center = 0;
        let preview_buf = self.preview_buf.lock().unwrap();
        let source = match self.preview_type {
            PreviewType::Preview => preview_buf.get_preview(x_max * 2),
            PreviewType::LivePreview => preview_buf.get_live_preview(
                x_max * 2,
                self.player_pos,
                playhead_offset_from_center,
            ),
        };
        // println!("x:({},{}), y:({}{})", x_min, x_max, y_min, y_max);
        // println!("preview_buf_len: {}", preview_buf.len());
        let canvas = Canvas::default()
            .block(Block::default().borders(Borders::BOTTOM))
            .x_bounds([-(x_max as f64), x_max as f64])
            .y_bounds([-(y_max as f64), y_max as f64])
            .paint(|ctx| {
                let mut prev = 0.0 as f32;
                // center line
                if self.preview_type == PreviewType::LivePreview {
                    ctx.draw(&Line {
                        x1: -(playhead_offset_from_center as f64),
                        x2: -(playhead_offset_from_center as f64),
                        y1: -(y_max as f64),
                        y2: y_max as f64,
                        color: Color::Red,
                    });
                }
                ctx.layer();
                // for i in (1..(area.width as usize)) {
                for (i, sample) in source
                    .to_owned()
                    .into_iter()
                    .take(x_max * 2 as usize)
                    .enumerate()
                {
                    // determine x
                    // let x = ((i * chunk_size) as f32) - (preview_buf_len as f32 / 2.0);
                    // fit sample (a value between 0 and 1) into area height
                    let x = (-(x_max as i16) + i as i16) as f64;
                    let y = (sample * (y_max as f32)) as f64;
                    if self.preview_type == PreviewType::LivePreview {
                        let y = 5. * y;
                    } else {
                        let y = 20.0 * y;
                    }
                    // draw line
                    ctx.draw(&Line {
                        x1: x,
                        x2: x,
                        y1: y,
                        y2: -y,
                        color: Color::Gray,
                    });
                    // draw inner line
                    ctx.draw(&Line {
                        x1: x,
                        x2: x,
                        y1: y * 0.4,
                        y2: -y * 0.4,
                        color: Color::DarkGray,
                    });
                    prev = sample;
                }
            });
        canvas.render(area, buf);
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
