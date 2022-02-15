use std::hash::Hash;
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::core::player::PreviewBuffer;

#[derive(Debug)]
//------------------------------------------------------------------//
//                              Track                               //
//------------------------------------------------------------------//
pub struct Track {
    pub meta: TrackMeta,
    pub file_path: String,
    pub file_name: String,
    pub preview_buffer: Arc<Mutex<PreviewBuffer>>,
}

impl Track {
    pub fn new(file_path: String) -> Self {
        let file_name = String::from(Path::new(&file_path).file_name().unwrap().to_str().unwrap());
        Self {
            meta: TrackMeta::default(),
            preview_buffer: Arc::new(Mutex::new(PreviewBuffer::default())),
            file_path,
            file_name,
        }
    }
}

impl Eq for Track {}

impl PartialOrd for Track {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.file_path.partial_cmp(&other.file_path)
    }
}
impl PartialEq for Track {
    fn eq(&self, other: &Self) -> bool {
        self.file_path == other.file_path
    }
}

impl Ord for Track {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.file_path.cmp(&other.file_path)
    }
}

impl Hash for Track {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.file_path.hash(state)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct TrackMeta {}
impl Default for TrackMeta {
    fn default() -> Self {
        Self {}
    }
}
