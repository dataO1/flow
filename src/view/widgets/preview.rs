use tui::{
    style::Color,
    widgets::{
        canvas::{Canvas, Line},
        Block, Borders, Widget,
    },
};

use crate::view::model::track::Track;

pub struct PreviewWidget<'a> {
    track: &'a Track,
    player_position: usize,
}

impl<'a> PreviewWidget<'a> {
    pub fn new(track: &'a Track, player_position: usize) -> Self {
        Self {
            track,
            player_position,
        }
    }
}

impl<'a> Widget for PreviewWidget<'a> {
    fn render(self, area: tui::layout::Rect, buf: &mut tui::buffer::Buffer) {
        let x_max = area.width as usize;
        let y_max = area.height as usize;
        let n_packets = self.track.n_packets().unwrap_or(1);
        let playhead_position = (self.player_position as f64 / n_packets as f64) * (x_max as f64);
        let playhead_position = (playhead_position as i64) - (x_max as i64);
        let canvas = Canvas::default()
            .block(Block::default().borders(Borders::BOTTOM))
            .x_bounds([-(x_max as f64), x_max as f64])
            .y_bounds([-(y_max as f64), y_max as f64])
            .paint(|ctx| {
                //
                for (i, sample) in self
                    .track
                    .preview(x_max * 2)
                    .to_owned()
                    .into_iter()
                    .take((x_max * 2) as usize)
                    .enumerate()
                {
                    //
                    let x = (-(x_max as i16) + i as i16) as f64;
                    let y = (sample * (y_max as f32)) as f64;
                    let y = y * 10.;
                    // // clip the signal if too hight
                    // let y = if y > (y_max as f64) { y_max as f64 } else { y };
                    ctx.draw(&Line {
                        x1: x,
                        x2: x,
                        y1: y,
                        y2: -y,
                        color: Color::Gray,
                    });
                }
                ctx.layer();
                ctx.draw(&Line {
                    x1: playhead_position as f64,
                    x2: playhead_position as f64,
                    y1: y_max as f64,
                    y2: -(y_max as f64),
                    color: Color::Red,
                })
            });
        canvas.render(area, buf);
    }
}
