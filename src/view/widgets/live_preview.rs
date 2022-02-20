use std::collections::VecDeque;
use tui::buffer::Buffer;
use tui::layout::Rect;
use tui::style::Color;
use tui::widgets::canvas::Context;
use tui::widgets::{
    canvas::{Canvas, Line},
    Block, Borders, Widget,
};

use crate::core::analyzer::PreviewSample;
use crate::core::player::TimeMarker;
use crate::view::model::track::Track;

pub struct LivePreviewWidget<'a> {
    track: &'a Track,
    player_pos: &'a Option<TimeMarker>,
}

pub enum WaveFormLayer {
    Lows,
    Mids,
    Highs,
}

impl<'a> LivePreviewWidget<'a> {
    pub fn new(track: &'a Track, player_pos: &'a Option<TimeMarker>) -> Self {
        Self { player_pos, track }
    }

    pub fn draw_waveform(
        &self,
        ctx: &mut Context,
        layer: WaveFormLayer,
        target_size: usize,
        y_max: usize,
    ) {
        if let Some(player_pos) = self.player_pos {
            for (i, sample) in self
                .track
                .live_preview(target_size, player_pos)
                .into_iter()
                .take(target_size)
                .enumerate()
            {
                let x = (-((target_size / 2) as i32) + i as i32) as f64;
                let y = match layer {
                    WaveFormLayer::Lows => sample.lows,
                    WaveFormLayer::Mids => sample.mids,
                    WaveFormLayer::Highs => sample.highs * 2.,
                };
                let y = (y * (y_max as f32)) as f64;
                let color = match layer {
                    WaveFormLayer::Lows => Color::Red,
                    WaveFormLayer::Mids => Color::Green,
                    WaveFormLayer::Highs => Color::White,
                };
                ctx.draw(&Line {
                    x1: x,
                    x2: x,
                    y1: y,
                    y2: -y,
                    color,
                });
            }
        }
    }
}

impl<'a> Widget for LivePreviewWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // this determines how many samples are "chunked" and thus displayed together as one line,
        // to fit the resolution of the given area
        let x_max = area.width as usize;
        let y_max = area.height as usize;
        let playhead_offset_from_center = 0;
        let target_size = x_max * 2;
        // println!("x:({},{}), y:({}{})", x_min, x_max, y_min, y_max);
        // println!("preview_buf_len: {}", preview_buf.len());
        let canvas = Canvas::default()
            .block(Block::default())
            .x_bounds([-(x_max as f64), x_max as f64])
            .y_bounds([-(y_max as f64), y_max as f64])
            .paint(|ctx| {
                // playhead
                ctx.draw(&Line {
                    x1: -(playhead_offset_from_center as f64),
                    x2: -(playhead_offset_from_center as f64),
                    y1: -(y_max as f64),
                    y2: y_max as f64,
                    color: Color::Red,
                });
                // ctx.layer();
                // self.draw_waveform(ctx, WaveFormLayer::Highs, target_size, y_max);
                // ctx.layer();
                self.draw_waveform(ctx, WaveFormLayer::Lows, target_size, y_max);
                self.draw_waveform(ctx, WaveFormLayer::Mids, target_size, y_max);
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
