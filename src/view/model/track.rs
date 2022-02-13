use std::sync::{Arc, Mutex};

use crate::core::player::PreviewBuffer;

#[derive(Debug)]
pub struct Track {
    pub meta: TrackMeta,
    pub file_path: String,
    pub preview_buffer: Arc<Mutex<PreviewBuffer>>,
}

impl Track {
    pub fn new(file_path: String) -> Self {
        Self {
            meta: TrackMeta::default(),
            preview_buffer: Arc::new(Mutex::new(PreviewBuffer::default())),
            file_path,
        }
    }
}

#[derive(Debug)]
pub struct TrackMeta {}
impl Default for TrackMeta {
    fn default() -> Self {
        Self {}
    }
}
