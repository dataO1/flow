use std::collections::VecDeque;
use tui::buffer::Buffer;
use tui::layout::Rect;
use tui::style::Color;
use tui::widgets::Widget;

pub struct WaveWidget<'a> {
    waveform: &'a DataBuffer,
}

impl<'a> WaveWidget<'a> {
    pub fn new(waveform: &'a DataBuffer) -> Self {
        Self { waveform }
    }
}

impl<'a> Widget for WaveWidget<'a> {
    /// Draws the WaveWidget's waveform onto the terminal buffer
    fn render(self, area: Rect, buf: &mut Buffer) {
        let Rect { width, height, .. } = area;
        let waveform_len = self.waveform.len();
        assert!(waveform_len > width.into());

        for col in 1..=width {
            buf.get_mut(col, height / 2)
                .set_char('=')
                .set_fg(Color::Green);
        }

        for (index, &sample) in self
            .waveform
            .iter()
            .skip(waveform_len - usize::from(width))
            .enumerate()
        {
            let col = index as u16 + 1;
            // Scale (might clip) sample to see more
            let norm_y = sample * 5.;

            let row = ((norm_y + 0.5) * f32::from(height)).floor() as u16;

            // If would clip, don't render anything
            if row > 0 && row < height {
                buf.get_mut(col, row).set_char('#').set_fg(Color::Cyan);
            }
        }
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
    pub fn push_latest_data(&mut self, buf_data: &[Sample]) {
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
