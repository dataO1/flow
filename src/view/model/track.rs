use std::collections::hash_map::DefaultHasher;
use std::hash::Hash;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use indexmap::set::IntoIter;
use indexmap::{IndexMap, IndexSet};

use crate::core::player::PreviewBuffer;

#[derive(Debug)]
//------------------------------------------------------------------//
//                              Track                               //
//------------------------------------------------------------------//
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

impl PartialEq for Track {
    fn eq(&self, other: &Self) -> bool {
        self.file_path == other.file_path
    }
}

impl Eq for Track {}
impl Hash for Track {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.file_path.hash(&mut DefaultHasher::new())
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct TrackMeta {}
impl Default for TrackMeta {
    fn default() -> Self {
        Self {}
    }
}
