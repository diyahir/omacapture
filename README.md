# Grabbit

Screenshot capture and annotation for Wayland compositors, built for Hyprland. Written in Rust with GTK4 and libadwaita.

- **Capture**: area, window, and fullscreen through `grim`, with a frozen-screen overlay that shows a magnifier, live size readout, window highlighting (`A` toggles window mode), Shift for square regions, arrow-key nudging, and Enter to reuse the last region.
- **Annotate**: selection, crop (aspect presets, edge snapping, auto-crop), rectangle, filled rectangle, oval, arrow (straight/curved, classic/tapered/outlined, per-end heads), line, text (plain/label/callout), highlighter with OCR text snapping, blur (pixelate, gaussian, hexagonal, crystallize, pointillism, halftone, tape, washi), spotlight, counters, watermark, pencil. Undo/redo, copy/paste/duplicate, zoom and pan, canvas backgrounds (gradients, solid, blurred, image) with padding, rounded corners, and shadow.
- **Auto-redact**: one click pixelates emails, phone numbers, URLs, credit-card numbers, API tokens, and `key=value` credentials found by local OCR.
- **Editable sessions**: every save keeps the original pixels and annotations so a screenshot re-opens with everything still editable.
- **Quick Access**: floating card after every capture with copy, edit, drag-to-app, open, and delete, plus hover shortcuts (`c`, `e`, `o`, `Delete`).
- **History**: searchable browser of recent captures with retention policies.
- **OCR**: region-to-clipboard text capture with tesseract.
- **MCP server**: lets AI agents capture, inspect, OCR, and annotate through the Model Context Protocol.
- **Config**: TOML at `~/.config/grabbit/config.toml`, editable through the Preferences window or by hand.

## Install

```sh
cargo build --release
install -Dm755 target/release/grabbit ~/.local/bin/grabbit
```

Runtime dependencies: `gtk4`, `libadwaita`, `gtk4-layer-shell`, `grim`, `wl-clipboard`; optional `tesseract` plus a language pack for OCR and highlighter text snapping, `canberra-gtk-play` for the shutter sound.

## Usage

```sh
grabbit area              # select a region
grabbit area --annotate   # select a region and annotate before saving
grabbit window            # pick a window
grabbit full              # every monitor
grabbit ocr               # region -> text on the clipboard
grabbit annotate FILE     # open an image in the editor
grabbit history           # capture history
grabbit settings          # preferences
grabbit daemon            # keep a resident instance so hotkeys respond instantly
grabbit --wait area       # independent instance; prints a JSON result line when done
```

Hyprland owns global shortcuts. Add to `~/.config/hypr/bindings.conf` (Preferences → Shortcuts has a copy button):

```
bind = , PRINT, exec, grabbit area
bind = SHIFT, PRINT, exec, grabbit window
bind = CTRL, PRINT, exec, grabbit full
bind = ALT, PRINT, exec, grabbit area --annotate
bind = SUPER SHIFT, T, exec, grabbit ocr
exec-once = grabbit daemon
```

### Editor keys

| Keys | Action |
| --- | --- |
| `V C R F O A L T H B S N W P` | Select, Crop, Rectangle, Filled, Oval, Arrow, Line, Text, Highlighter, Blur, Spotlight, Counter, Watermark, Pencil |
| `Ctrl+Z` / `Ctrl+Shift+Z` | Undo / redo |
| `Ctrl+S` / `Ctrl+Shift+C` / `Ctrl+E` | Save / copy and close / export as |
| `Ctrl+C` `Ctrl+V` `Ctrl+D` `Ctrl+A` | Copy, paste, duplicate, select all annotations (`Ctrl+C` with nothing selected copies the image) |
| `Ctrl+scroll`, `Ctrl+0`, `Ctrl+1`, Space+drag | Zoom, fit, actual size, pan |
| `Enter` / `Esc` / `A` while cropping | Apply / cancel / auto-crop to content |
| Shift while drawing | Square, circle, or 45° constraint |
| `Ctrl+B` | Toggle the canvas/background sidebar |

## MCP server for agents

`grabbit mcp` speaks the Model Context Protocol over stdio and needs no display of its own. Register it in Claude Code (the repo ships a `.mcp.json`) or any MCP client:

```json
{ "mcpServers": { "grabbit": { "command": "grabbit", "args": ["mcp"] } } }
```

Tools:

| Tool | What it does |
| --- | --- |
| `list_monitors`, `list_windows` | Discover outputs and visible windows (Hyprland) |
| `capture_screen`, `capture_area`, `capture_window` | Grab pixels headlessly; returns the saved path plus a downscaled image |
| `capture_interactive` | Ask the human to pick a region or window; blocks until they finish |
| `ocr` | Text (or word boxes) from a file or screen area |
| `annotate` | Draw rectangles, arrows, labels, callouts, blur, highlights, counters, spotlight, watermark, or pencil paths onto an image and write the result, with optional crop and canvas background |
| `history_list`, `read_image`, `open_editor` | Browse recent captures, look at a file, or hand an image to the human in the editor |

Example `annotate` call:

```json
{
  "path": "shot.png",
  "background": "blue purple",
  "items": [
    {"type": "rect", "x": 40, "y": 40, "width": 300, "height": 120, "color": "#ff3b30"},
    {"type": "arrow", "x1": 400, "y1": 300, "x2": 330, "y2": 150, "kind": "tapered"},
    {"type": "label", "x": 420, "y": 310, "text": "Look here", "font_size": 24},
    {"type": "blur", "x": 500, "y": 50, "width": 250, "height": 100}
  ]
}
```

## Development

```sh
cargo test            # model, redaction, and editor gesture tests (needs a Wayland display)
cargo build           # debug build
```

See `docs/SPEC.md` for the full behavioral specification.
