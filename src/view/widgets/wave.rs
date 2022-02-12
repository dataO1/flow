use itertools::Itertools;
use std::collections::VecDeque;
use tui::buffer::Buffer;
use tui::layout::Rect;
use tui::style::Color;
use tui::widgets::canvas::{Canvas, Line};
use tui::widgets::{Block, Borders, Widget};

use crate::core::player::WavePreview;

pub struct WaveWidget<'a> {
    waveform: &'a WavePreview,
}

impl<'a> WaveWidget<'a> {
    pub fn new(waveform: &'a WavePreview) -> Self {
        Self { waveform }
    }

    /// tries to detect transients and gives them color
    fn get_col(&self, prev: Sample, curr: Sample) -> Color {
        let diff = curr - prev;
        // try to detect transient
        if diff > 0.6 {
            Color::Red
        } else {
            Color::Green
        }
    }
}

impl<'a> Widget for WaveWidget<'a> {
    fn render(mut self, area: Rect, buf: &mut Buffer) {
        // println!("{:#?}", area);
        // let samples = samples.iter().cycle().take(2).collect();

        // this determines how many samples are "chunked" and thus displayed together as one line,
        // to fit the resolution of the given area
        let waveform_len = self.waveform.len();
        if waveform_len < area.width as usize {
            return;
        }
        let chunk_size = waveform_len / area.width as usize;
        let can = Canvas::default()
            .block(Block::default().title("Live Preview").borders(Borders::ALL))
            .x_bounds([-(waveform_len as f64 / 2.0), (waveform_len as f64 / 2.0)])
            .y_bounds([-(area.height as f64), (area.height as f64)])
            .paint(|ctx| {
                let mut prev_sample = 0.0;
                ctx.layer();
                // for i in (1..(area.width as usize)) {
                for (i, sample) in self
                    .waveform
                    .iter()
                    // group samples into chunks
                    .chunks(chunk_size)
                    .into_iter()
                    // and compute their average
                    .map(|chunk| {
                        let mut num = 0;
                        let mut sum = 0.0;
                        for samp in chunk {
                            num += 1;
                            sum += samp;
                        }
                        sum / num as f32
                    })
                    .enumerate()
                {
                    // determine x
                    let x = ((i * chunk_size) as f32) - (waveform_len as f32 / 2.0);
                    // fit sample (a value between 0 and 1) into area height
                    let y = sample * (area.height as f32);
                    // let y = y / 5.0;
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
                        color: self.get_col(sample, prev_sample),
                    });
                    prev_sample = sample;
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
