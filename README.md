# Art Slideshow (Rust + egui)

A lightweight fullscreen painting slideshow written in Rust using `eframe::egui`.  
Displays high-resolution artworks with automatic scaling, blurred background fill, and metadata loaded from JSON.

---

## Features

- 📁 **Folder-based slideshow**
  - Loads `jpg`, `jpeg`, `png`, `bmp`, `gif`
- 📝 **Per-image JSON metadata**
  - `title`, `artist`, `year`
  - Fallbacks to `"Unknown"` when missing
- 🖼 **Auto-scaling foreground image**
  - Fits screen while preserving aspect ratio
- 🌫 **Blurred background renderer**
  - Darkened, multi-pass custom blur
- ⚡ **Smooth playback**
  - Preloads next slide in a background thread
  - Zero stutter transitions
- 🌓 **Overlay text box**
  - Clean, readable info panel with metadata

---

## Screenshot Example

Here is an example of how the slideshow displays an artwork:

<img src="assets/slideshow_example1.png" width="700">
---
<img src="assets/slideshow_example2.png" width="700">

Foreground image is centered and scaled, while the blurred background fills the screen.

---

## Usage

### Run from source

```bash
cargo run --release -- "/path/to/folder"
