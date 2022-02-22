use tui::buffer::Buffer;
use tui::layout::Rect;
use tui::style::Color;
use tui::widgets::canvas::Context;
use tui::widgets::{
    canvas::{Canvas, Line},
    Block, Widget,
};

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
                .live_preview(target_size, 200, player_pos)
                .iter()
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
                    WaveFormLayer::Lows => Color::LightRed,
                    WaveFormLayer::Mids => Color::Gray,
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
                self.draw_waveform(ctx, WaveFormLayer::Lows, target_size, y_max);
                self.draw_waveform(ctx, WaveFormLayer::Mids, target_size, y_max);
                // self.draw_waveform(ctx, WaveFormLayer::Highs, target_size, y_max);
            });
        canvas.render(area, buf);
    }
}
