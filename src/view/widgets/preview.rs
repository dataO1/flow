use tui::{
    style::Color,
    widgets::{
        canvas::{Canvas, Line},
        Block, Widget,
    },
};

use crate::{core::player::TimeMarker, view::model::track::Track};

pub struct PreviewWidget<'a> {
    track: &'a Track,
    player_position: &'a Option<TimeMarker>,
}

impl<'a> PreviewWidget<'a> {
    pub fn new(track: &'a Track, player_position: &'a Option<TimeMarker>) -> Self {
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
        let preview_buffer = &self.track.preview(x_max * 2);

        let canvas = Canvas::default()
            .block(Block::default())
            .x_bounds([-(x_max as f64), x_max as f64])
            .y_bounds([-(y_max as f64), y_max as f64])
            .paint(|ctx| {
                //
                for (i, sample) in preview_buffer.iter().take((x_max * 2) as usize).enumerate() {
                    //
                    let x = (-(x_max as i16) + i as i16) as f64;
                    let y = (sample.lows * (y_max as f32)) as f64;
                    // let y = 1. * 20.;
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

                if let Some(player_position) = self.player_position {
                    let relative_pos = player_position.get_progress();
                    let x = relative_pos * x_max as f64 * 2.0;
                    let x = x.floor() as isize - x_max as isize;
                    ctx.draw(&Line {
                        x1: x as f64,
                        x2: x as f64,
                        y1: y_max as f64,
                        y2: -(y_max as f64),
                        color: Color::Red,
                    })
                }
                for marker in &(*self.track.mem_cues.lock().unwrap()) {
                    let relative_pos = marker.get_progress();
                    let x = relative_pos * x_max as f64 * 2.0;
                    let x = x.floor() as isize - x_max as isize;
                    ctx.draw(&Line {
                        x1: x as f64,
                        x2: x as f64,
                        y1: y_max as f64,
                        y2: -(y_max as f64),
                        color: Color::Green,
                    });
                }
            });
        canvas.render(area, buf);
    }
}
