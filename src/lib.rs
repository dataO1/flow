pub mod core;
pub mod view;

#[derive(Clone, Debug)]
pub enum Event {
    TogglePlay,
    LoadTrack(String),
    Quit,
    SamplePlayed(Vec<f32>),
    Unknown,
}
