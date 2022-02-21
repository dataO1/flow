# Introduction
Flow is a terminal music player, analyzer and cue point editor that aims to
replace Rekordbox, Denon's Engine Prime, SeratoDJ etc. by being platform agnostic. Currently only the basic player
capabilities are implemented, see [Roadmap](#roadmap).
# <a name="roadmap"></a>Roadmap
  - UI ([tui-realm]( https://github.com/veeso/tui-realm ) or [tui-rs](https://github.com/fdehau/tui-rs)) 
    * [x] Mockup/Design
    * [ ] Waveform - Overview
    * [x] Waveform - Live Preview
    * [x] File list viewer
    * [ ] Playlists Editor
  - Player ([Symphonia](https://crates.io/crates/symphonia))
    * [x] Async Event handling
    * [x] Play/Pause
    * [x] Seeking
    * [x] Cue
  - Track Analysis
    * [ ] BPM Detection (Aubio)
    * [ ] Beatgrid Detection (Aubio)
    * [x] Waveform
    * [ ] Creating cue points
  - Data Import/Export
    * [ ] Storing and loading track analysis results
    * [ ] Exporting data in the following formats:
      * [ ] Rekordbox with [rekordcrate](https://github.com/Holzhaus/rekordcrate)
      * [ ] Denon engine prime
      * [ ] SeratoDJ


# Interesting Libs
## Serialization/Deserialization 
  - [Bincode](https://github.com/bincode-org/bincode), since Kaitai doesn't support serialization yet, we're bound to a rust native solution
  - [Kaitai](https://kaitai.io/#what-is-it) for serializing rekordbox file formats. There are also [Kaitai rust bindings]( https://github.com/kaitai-io/kaitai_struct_rust_runtime) and a [Kaitai-like serialization crate](https://github.com/manuels/taikai), but these seem to be outdated.
  - [rekordcrate](https://github.com/Holzhaus/rekordcrate) -> This looks the most promising of a solution!
## Audio Analysis
  - [Aubio](https://docs.rs/aubio/latest/aubio/) for BPM, Beatgrid, Onset-detection and more
## TUI 
  - [tui-rs](https://github.com/fdehau/tui-rs) for genereal TUI implementation
## Playlists/Crates
  - [Bliss](https://rustrepo.com/repo/Polochon-street-bliss-rs-rust-audio-and-music for smart playlists)


# Further links
## Rekordbox file formats
  - [Kaitai rekordbox database format](https://github.com/Deep-Symmetry/crate-digger/blob/main/src/main/kaitai/rekordbox_pdb.ksy), more info [here](https://djl-analysis.deepsymmetry.org/rekordbox-export-analysis/exports.html)
