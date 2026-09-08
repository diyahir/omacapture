# Grabbit

Screenshot capture and annotation for Wayland compositors, built for Hyprland.

- Area, window, and fullscreen capture through `grim`, with a frozen-screen selection overlay
- Annotation editor: shapes, arrows, text, highlighter with text snapping, blur, spotlight, counters, watermark, crop, backgrounds
- Quick Access floating card after every capture: copy, edit, drag to any app, open, delete
- Capture history browser, OCR text capture (tesseract), TOML configuration at `~/.config/grabbit/config.toml`

## Build

```sh
cargo build --release
```

Runtime dependencies: `gtk4`, `libadwaita`, `gtk4-layer-shell`, `grim`, `wl-clipboard`; optional `tesseract` for OCR.

## Usage

```sh
grabbit area              # select a region
grabbit area --annotate   # select a region and annotate before saving
grabbit window            # pick a window
grabbit full              # every monitor
grabbit ocr               # region -> text on the clipboard
grabbit annotate FILE     # open an image in the editor
grabbit history           # capture history
grabbit daemon            # keep a resident instance so hotkeys respond instantly
```

Add to `~/.config/hypr/bindings.conf`:

```
bind = , PRINT, exec, grabbit area
bind = SHIFT, PRINT, exec, grabbit window
bind = CTRL, PRINT, exec, grabbit full
bind = ALT, PRINT, exec, grabbit area --annotate
exec-once = grabbit daemon
```

See `docs/SPEC.md` for the full feature specification.
