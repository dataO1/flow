use std::hash::Hash;
use std::path::Path;
use std::sync::{Arc, Mutex};

use itertools::Itertools;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::CodecParameters;

use crate::core::analyzer::PREVIEW_SAMPLES_PER_PACKET;
use crate::core::player::PreviewBuffer;

//------------------------------------------------------------------//
//                              Track                               //
//------------------------------------------------------------------//

#[derive(Debug)]
pub struct Track {
    /// track meta data
    pub meta: TrackMeta,
    /// codec parameters
    pub codec_params: CodecParameters,
    /// the file path
    pub file_path: String,
    /// the file name
    pub file_name: String,
    /// downsampled version of decoded frames for preview
    preview_buffer: Mutex<Vec<f32>>,
}

impl Track {
    pub fn new(file_path: String, codec_params: CodecParameters) -> Self {
        let file_name = String::from(Path::new(&file_path).file_name().unwrap().to_str().unwrap());
        Self {
            meta: TrackMeta::default(),
            preview_buffer: Mutex::new(vec![]),
            file_path,
            file_name,
            codec_params,
        }
    }

    /// append preview samples to preview buffer
    pub fn append(&self, preview_samples: &mut Vec<f32>) {
        // Hack: this sets the frames per packet
        // if self.avg_frames_per_packet == None {
        //     self.avg_frames_per_packet = Some((samples.len() / 2) as u64);
        // }
        // since the samples in the packets are interlaeved (2 channels), we have to adjust the
        // chunk size
        self.preview_buffer.lock().unwrap().append(preview_samples);
    }

    pub fn preview(
        &self,
        target_size: usize,
        player_position: usize,
        playhead_position: usize,
    ) -> Vec<f32> {
        let preview_buffer = &mut *self.preview_buffer.lock().unwrap();
        let player_pos = player_position * PREVIEW_SAMPLES_PER_PACKET;
        // check if enough sampes exist for target resolution
        let diff = player_pos as isize - (target_size as isize / 2);
        if diff >= 0 {
            // if yes return buffer content
            let l = player_pos as f32 - (target_size as f32 / 2.0);
            let r = player_pos as f32 + (target_size as f32 / 2.0);
            preview_buffer[l as usize..r as usize].to_owned()
        } else {
            let diff = diff.abs() as usize;
            let mut padding = vec![0.0 as f32; diff];
            padding.append(preview_buffer);
            padding.to_owned()
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
