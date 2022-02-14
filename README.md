# Current State and TODO
  - UI ([tui-realm]( https://github.com/veeso/tui-realm ) or [tui-rs](https://github.com/fdehau/tui-rs)) 
    * [x] Test UI, that handles key events and triggers player
    * [x] Mockup/Design Paper
    * [-] Waveform - Overview
    * [x] Waveform - Live
    * [x] File list viewer
    * [ ] Playlists Editor
  - Player ([Symphonia](https://crates.io/crates/symphonia))
    * [x] Async Event handling
    * [x] Play/Pause
    * [ ] Seeking
    * [-] Cue
  - Track Analysis
    * [ ] BPM Detection (Aubio)
    * [ ] Beatgrid Detection (Aubio)
    * [x] Waveform
    * [ ] Creating cue points
    * [ ] Storing results in a mobile database
      *  which DB to use?
  - Data Export
    * [ ] Converting internal structure into [rekordcrate's](https://github.com/Holzhaus/rekordcrate) structure
    * [ ] Exporting data

# Ideas
  - TUI with vim bindings, wave form and overview of tracks/playlists/crates 
  - export: export data in rekordbox format, since all major players support
    this format
  - try to make playlists generic, such that smart algorithms/AI can generate
    playlists/analyze tracks somehow
  - save track information/playlists etc in some intermediate open format, which
    can be stored in git and is human readable and can be exported as rekordbox
    database
  - analyze and edit track bpm, grid, pitch and key.
  - drop detection (https://musicmachinery.com/2015/06/16/the-drop-machine/)

# Libs
## Serialization/Deserialization 
  - [Bincode](https://github.com/bincode-org/bincode), since Kaitai doesn't support serialization yet, we're bound to a rust native solution
  - [Kaitai](https://kaitai.io/#what-is-it) for serializing rekordbox file formats. There are also [Kaitai rust bindings]( https://github.com/kaitai-io/kaitai_struct_rust_runtime) and a [Kaitai-like serialization crate](https://github.com/manuels/taikai), but these seem to be outdated.
  - [rekordcrate](https://github.com/Holzhaus/rekordcrate) -> This looks the most promising of a solution!
## Audio Analysis
  - [Aubio](https://docs.rs/aubio/latest/aubio/) for BPM, Beatgrid, Onset-detection and more
## TUI 
  - [tui-rs](https://github.com/fdehau/tui-rs) for genereal TUI implementation
  - [live preview of wave audio form](https://github.com/jeffvandyke/rust-tui-audio) for tui-rs
## Playlists/Crates
  - [Bliss] https://rustrepo.com/repo/Polochon-street-bliss-rs-rust-audio-and-music for smart playlists

# Further links
## Rekordbox file formats
  - [Kaitai rekordbox database format](https://github.com/Deep-Symmetry/crate-digger/blob/main/src/main/kaitai/rekordbox_pdb.ksy), more info [here](https://djl-analysis.deepsymmetry.org/rekordbox-export-analysis/exports.html)

# Current Problems
  - Kaitai doesnt support serialization, also the rust bindings are not available in cargo, nor do they seem to be maintained very well.
