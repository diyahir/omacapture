# Omashot — Behavioral Specification

Omashot is a Linux (Rust + GTK4, Wayland/Hyprland) screenshot and annotation tool. This document
describes *what the user experiences*, not how it is built. It is derived from studying the
behavior of a macOS screenshot app and re-expressed independently for a Linux desktop. Scope is
limited to still-image capture, the post-capture flow, the annotation editor, and capture
history. Explicitly out of scope: video recording, video/GIF editing, cloud upload, in-app
updaters, scrolling capture.

Platform mapping used throughout (macOS concept -> Linux equivalent):

| macOS | Omashot on Linux |
| --- | --- |
| ScreenCaptureKit / CGDisplay capture | `wlr-screencopy` / `ext-image-copy-capture` via `grim`-style client, or xdg-desktop-portal Screenshot as fallback |
| Window enumeration (CGWindowList) | Hyprland IPC (`hyprctl clients -j`) / `wlr-foreign-toplevel`; portal has no per-window pick, so window capture crops the window rect from a display copy |
| Vision OCR / barcode | `tesseract` (leptonica) + `zbar` for QR |
| Vision subject mask (object cutout) | later; `rembg`/ONNX if ever added |
| Keychain | `libsecret` (only needed if a remote OCR key is ever stored) |
| UserDefaults | TOML config (`~/.config/omashot/config.toml`) + a small state file |
| NSPasteboard | `wl-clipboard` semantics via GTK4 `Gdk.Clipboard` (image/png + text/uri-list) |
| Carbon global hotkeys | Hyprland `bind` lines invoking `omashot --capture area` (CLI/D-Bus), plus GlobalShortcuts portal where available |
| snapzy:// URL scheme | `omashot` CLI subcommands and a D-Bus service; same verbs |
| NSPanel floating, all-Spaces | `gtk4-layer-shell` overlay surfaces (layer `overlay`, exclusive keyboard while selecting) |
| Native notifications | `org.freedesktop.Notifications` (libnotify) |
| Application Support | `$XDG_DATA_HOME/omashot/` (captures cache, sessions, thumbnails, sqlite) |

---

## 1. Capture modes

| Mode | What it produces | Notes |
| --- | --- | --- |
| Fullscreen | One image per targeted display | Default targets only the display under the pointer; an "all displays" variant produces a batch. |
| Area | Cropped image of a user-drawn rectangle | May span monitors. |
| Window (toggle inside Area) | Exact window pixels | Press `A` (configurable) while the area overlay is up to switch to window-pick mode; also directly launchable. |
| Active window | The focused window, no overlay shown | Resolved via compositor focus. |
| Area + inline annotate | Annotated image | Draw the region, then annotate in place before anything is written. |
| OCR text | Plain text on the clipboard, no file | QR payloads are appended. |
| Object cutout | Transparent PNG of the subject | **Later.** Needs a segmentation model; listed for completeness. |

Every mode is launchable from: tray/status menu, a global shortcut, and the CLI/D-Bus verb.
Omashot's own windows are hidden for the duration of a capture unless "include own windows" is on.

### 1.1 Fullscreen

- Captures the display the pointer is on at trigger time. Multi-display batch mode captures every
  display into separate files; post-capture treats the batch as one event (see §2).
- Output is at the display's native pixel scale (a 2x display yields 2x pixels; a 1x display stays
  1x). No upscaling of 1x displays. When one output image is composed from displays of different
  scales, lower-scale slices are upsampled to the highest scale present so the composite is uniform.
- Optional: hide cursor (default off), hide desktop icons/widgets (no Linux equivalent for widgets;
  keep only "hide cursor").

### 1.2 Area selection overlay

The overlay is a fullscreen layer-shell surface per monitor sharing one global coordinate space, so a
single drag can cross monitor boundaries.

- **Live by default.** The screen underneath keeps updating while the user selects; the pixels are
  grabbed on mouse-up. An opt-in "freeze" mode snapshots every display first and selects on the
  frozen image (useful for capturing transient UI). Config: `capture.screenshot.freeze_area`.
- **Dimming.** With "show selection area overlay" on (default), the whole screen is darkened and the
  dragged rectangle is a bright cutout. With it off, nothing is dimmed and only the rectangle outline
  and readouts are drawn (the outline auto-flips light/dark based on the luminance beneath it).
- **Cursor.** Crosshair in area mode, a camera glyph in window mode. The compositor cursor is hidden
  and a proxy cursor is drawn by the overlay so the appearance is consistent across apps.
- **Readouts.** While hovering: pointer coordinates in a small bubble near the cursor. While dragging:
  the bubble switches to `W x H` in logical pixels, anchored just outside the rectangle.
- **Magnifier.** A ~130 px circular loupe next to the cursor shows the pixels under it with a center
  crosshair. Scroll wheel changes zoom 1x–20x; direction is reversible in preferences. Optional
  color-value panel (hex/RGB of the center pixel) defaults on; "show magnifier by default" defaults
  off (the loupe appears while dragging or when toggled).
- **Confirm / cancel.**
  - Mouse-up ends the drag and captures immediately.
  - `Esc` or right-click cancels; nothing is written.
  - `Enter` before dragging = "repeat last area" (see below). Omashot also treats `Enter`/`Space`
    after a drag-in-progress-with-handles as confirm.
- **Space-drag move.** Holding `Space` mid-drag moves the rectangle instead of resizing it (this is
  the behavior in the inline-annotate overlay; Omashot applies it to plain area capture too).
- **Constraint modifiers (Omashot addition — the reference app has none in the capture overlay).**
  `Shift` locks 1:1; `Ctrl` draws from center; arrow keys nudge the pending rectangle by 1 px
  (`Shift`+arrow: 10 px) before confirm with `Enter`.
- **Window-pick mode (`A`).** Hovering highlights the topmost window under the pointer with a cutout
  matching the window's frame; click captures that window exactly (decorations and shadow included
  where the compositor supplies them). Omashot's own windows are never candidates. Press `A` again
  to return to manual region.
- **Remember last area.** Every completed manual area selection is stored (global logical coords).
  A separate "repeat area" shortcut re-captures that rectangle instantly without showing the
  overlay; if the stored rect is off-screen or no display contains it, a notification says so and
  nothing happens. `Enter` inside a fresh overlay does the same.
- **Multi-monitor.** Selection is tracked in global coordinates and rendered on every monitor it
  touches. For a single-display selection the output display is the one with the largest overlap.
  For a multi-display selection each display is captured and stitched into one image at the
  highest native scale involved.
- **Hot-plug.** Displays attached mid-session get an overlay immediately; an overlay never becomes a
  click-through hole.

### 1.3 Window capture and active window

- Window mode enumerates compositor toplevels front-to-back; only normal windows and already-open
  popups/menus count. Popups that close when focus changes are captured from a pre-overlay display
  snapshot so they still appear.
- Active-window capture asks the compositor for the focused toplevel and captures it with no UI.
- On Linux, window pixels are cropped from a display copy unless the compositor supports per-toplevel
  copy; rounded-corner alpha is applied from the window geometry so corners export transparent.

### 1.4 Area + inline annotate

The area overlay stays up after the drag and turns into a mini editor around the selection:

- A toolbar with Selection + all drawable tools (§4.2) except Crop and Mockup, a quick-properties
  bar, and an action rail: **Pin**, **Cancel**, **Done** (prominent), **Copy**.
- The selection rectangle still has 8 resize handles and can be Space-dragged; annotations keep
  their positions relative to the image while the region changes.
- `Enter` / `Ctrl+S` / Done renders and saves through the normal post-capture pipeline; `Ctrl+C`
  copies the rendered image; `Esc` discards everything. Pin saves then opens the result in a pinned
  always-on-top window.
- No canvas preset auto-apply here (see §2.5).

### 1.5 OCR text capture

- Same overlay as area capture; on mouse-up the region is run through OCR (tesseract) and QR
  detection (zbar). A tray spinner indicates work in progress.
- Result goes to the clipboard as plain text; QR payloads are appended, deduplicated, and skipped
  if already contained in the OCR text. No file is written and no Quick Access card appears.
- Exactly one outcome notification is shown: "Text copied" + a one-line preview (max ~200 chars,
  whitespace collapsed, ellipsis), "No text detected", or "OCR failed". Fallback to an in-app toast
  if the notification daemon is unavailable. Config: `capture.ocr.success_notification` (default on).
- Optional link detection: up to 3 http(s) URLs found in the text are offered in a small floating
  prompt for 10 s (click opens in browser). Decoded QR URLs are never auto-opened.
- OCR provider is pluggable: `builtin` (tesseract) or `custom:<uuid>` (an OpenAI-compatible
  endpoint). Language packs are a preference; default follows system locale plus English.

### 1.6 Object cutout (later)

Subject segmentation to a transparent PNG with an optional safe auto-crop (skip when the subject is
tiny or the crop would save <8%). Always exports PNG regardless of the configured format.

---

## 2. Post-capture actions

### 2.1 Action matrix

Each after-capture action is an independent boolean, stored per capture kind. Omashot has one kind
(screenshot); the schema keeps the nesting so other kinds can be added later.

| Action | Default | Effect |
| --- | --- | --- |
| `save` | on | Write to the export folder. When off, write to the private captures cache instead and offer "Save" on the card. |
| `copy_file` | on | Put the image on the clipboard (image/png + file URI in one clipboard offer). |
| `quick_access` | on | Show a Quick Access card. |
| `open_annotate` | off | Auto-open the editor. |

Ordering is deliberate: (1) optional default canvas preset bake (§2.5), (2) clipboard copy first
so paste is never delayed by UI work, (3) Quick Access card, (4) optional pin (when requested by
inline annotate), (5) editor auto-open, (6) history record.

Batch (multi-display fullscreen): every file gets a card and a history record, the clipboard
receives the list of file URIs, and only the first file auto-opens in the editor.

OCR never enters this pipeline. Combinations are free-form: e.g. `save=off, copy=on, qa=off` gives
a pure "copy to clipboard" workflow with a temp file kept alive for paste-time reads.

After an edit is saved from the editor, the clipboard copy is re-run if `copy_file` is on.

### 2.2 Destinations

- **Export folder**: default `~/Pictures/Screenshots` (macOS reference uses Desktop; Pictures fits XDG).
- **Cache folder** (when `save=off`): `$XDG_DATA_HOME/omashot/captures/`. Deliberately not `/tmp`
  so drag-and-drop and paste never race a tmp cleaner. Saving later (Quick Access "Save") moves the
  file into the export folder preserving any template subfolders and moves its annotation session
  with it.
- Launch-time sweep removes orphaned cache files that have no history record and are outside the
  retention window.

### 2.3 File naming

Template string with tokens, default `Omashot_{datetime}_{ms}`:

`{datetime} {date} {time} {timestamp} {year} {yearShort} {month} {monthName} {monthShort} {day}
{ms} {type} {appName}` (snake_case aliases accepted: `{year_short} {month_name} {month_short}
{app_name} {yy}`).

- `/` in the template creates subfolders under the export folder; each segment is sanitized and
  `..` is dropped.
- `{appName}` is the captured window's app id (or the focused app for fullscreen/area), empty
  when unknown.
- Collisions get `_2`, `_3`, ... suffixes.
- A live preview of the resolved name is shown in preferences.

### 2.4 Formats and scale

- `png` (default), `jpg` (quality 0–100, default ~90), `webp` (lossy, quality ~80). Cutout and any
  image with alpha force PNG.
- DPI metadata is written as `scale x 72` (plus pixels-per-meter for PNG) so re-opening a 2x capture
  restores the correct logical size.
- Output pixels are at native display scale (never below 1x).

### 2.5 Default canvas preset

If a canvas preset (§4.9) is marked default, it is baked into every new screenshot (padding,
background, shadow, corner radius) *before* copy/QA, and an editable session is stored so the
capture reopens with the preset still adjustable. Inline annotate skips this.

---

## 3. Quick Access panel

A small floating stack of cards that appears after each capture.

- **Placement**: one of four corners (`bottom_right` default; UI exposes left/right on the bottom
  edge). The panel is pinned to the monitor the capture came from; a later capture on another
  monitor replays the slide-in there. It never follows the pointer.
- **Card**: 180x112 logical px base, scaled by `overlay_scale` 0.75–1.5 (step 0.25). Thumbnail,
  pin indicator, progress overlay when work is in flight.
- **Stack**: newest on top, max 5 visible, oldest evicted. Cards behind the top one scale down and
  fade slightly. 8 px spacing.
- **Entry animation**: 0.4 s spring slide from the screen edge (or scale); fade if reduced motion
  is set. Optional sound.
- **Auto-dismiss**: countdown 3–30 s (default 10), toggleable. Pauses on hover (default on), while
  the item is being edited, and while any job runs on it. Pinned cards never count down.
- **Hover UI**: card dims and reveals up to 2 text buttons in the center and up to 4 icon buttons
  in the corners (staggered reveal). Double-click opens the editor. Right-click shows a menu with
  the same actions, destructive last. Actions the item cannot use stay visible but disabled.
- **Actions** (7; each can be enabled/disabled, reordered, and assigned to a slot):

  | Action | Default slot | Behavior |
  | --- | --- | --- |
  | copy | center top | Clipboard copy then dismiss; cache file kept for paste. |
  | save / open | center bottom | Cache item: move to export folder. Saved item: reveal in file manager. |
  | dismiss | top right | Remove card; cache file deleted unless history references it or the clipboard still points at it. |
  | delete | top left | Delete file (trash if saved), history record, and annotation session. Confirm. |
  | edit | bottom left | Open in the annotation editor; pauses countdown. |
  | pin to screen | unassigned | Opens an always-on-top pin window. |
  | upload | — | Omitted in Omashot. |

- **Hover shortcuts**: while a card is hovered, `Ctrl+C` copy, `Ctrl+S` save/open, `Ctrl+E` edit,
  `Ctrl+P` pin, `Ctrl+Backspace` delete, `Ctrl+W` dismiss. Only exact bindings are consumed;
  everything else passes to the focused app. Master toggle plus per-action disable. A separate
  panel-wide "edit latest capture" (`Ctrl+Enter`, off by default) targets the newest card without
  hover.
- **Gestures**: mouse drag >30 px toward the screen edge = swipe-dismiss; drag away = drag-to-app
  (a file URI + image drag). Two-finger touchpad swipe (distance 80 / velocity 300, sensitivity
  0.5–3.0, natural/inverted) triggers the per-direction swipe action (both default dismiss).
- **Hide when editor open**: default on — cards hide while their editor is open and reappear on
  close with the updated thumbnail (instant low-res, then authoritative render).
- **Pin windows**: screenshots only. Always-on-top, zoom 0.4x–2x (scroll/pinch, preset menu),
  lock mode (click-through except an unlock button, image fades on hover), `Esc` closes when
  unlocked, drag-out handle exports the current render. Closing a pin unpins its card and restarts
  its countdown. Edits saved while pinned update the pin.
- **Suspension**: the panel ignores input during a capture session so it never intercepts the drag.

---

## 4. Annotation editor

### 4.1 Layout

Toolbar (tools) -> quick-properties bar (context controls for the active tool or selection) ->
main row: 240 px sidebar (canvas/background/effects) | canvas -> bottom bar. Three editor modes:
**annotate** (flat), **mockup** (3D tilt), **preview** (chrome hidden). Sidebar toggles with
`Ctrl+B`. Multiple editor windows can be open; each has its own zoom/pan/undo.

Bottom bar: zoom picker + mode switch (left), drag handle (center), and on the right: new window,
share (portal OpenURI / send-to), pin (`Ctrl+Alt+P`), copy & close (`Ctrl+Shift+C`), delete
(confirm; removes file, history, session, card).

### 4.2 Tools and shortcuts

| Tool | Key | Creation gesture | Quick props |
| --- | --- | --- | --- |
| Selection | `V` | click / marquee | — |
| Crop | `C` | enters crop mode | aspect presets, snap toggle |
| Rectangle | `R` | drag | color, width, corner radius, line style |
| Filled rectangle | `F` | drag | color, width, corner radius, line style |
| Oval | `O` | drag | color, width, line style |
| Arrow | `A` | drag | color, width, style, type, heads, line style |
| Line | `L` | drag | color, width, line style |
| Text | `T` | click to place, type | color, font size, presentation, corner radius |
| Highlighter | `H` | freehand drag | color, width, text-snap toggle |
| Blur | `B` | drag | effect type, strength, auto-redact button |
| Spotlight | `S` | drag | dim opacity, corner radius |
| Counter | `N` | click to place | color, size |
| Watermark | `W` | drag | text, style, opacity, rotation, color |
| Pencil | `P` | freehand drag | color, width |
| Mockup | `M` | switches to mockup mode | (sidebar) |

- Single-key tool shortcuts are rebindable and individually disableable. They are inactive while a
  text field has focus.
- Shape/arrow/line/blur/spotlight/watermark require an actual drag to create; a click does nothing.
  Counter and Text are click-to-place. Freehand tools commit on mouse-up.
- With a drawing tool active, dragging over an existing item creates a new item; use Selection to
  move/resize.
- Sessions start with `annotate.default_tool` (Selection). With `remember_last_tool` on, the last
  explicit tool choice is used instead.

### 4.3 Shared item properties and defaults

| Property | Default | Range / notes |
| --- | --- | --- |
| stroke color | red | palette + custom; favorites capped at 4 per role, custom colors 24 |
| fill color | clear | filled rectangle uses stroke color |
| stroke width | 3 | control value 1–20 |
| line style | solid | solid / dashed / dotted; dash length scales with width; dotted = round caps |
| corner radius | 0 | rectangle, filled rectangle, text label, spotlight |
| font size | 16 | text |
| font | system UI sans | text |
| opacity | 1 | watermark clamps to 0.05–0.65 |
| rotation | 0 | watermark clamps to -45..45 |
| spotlight dim | 0.5 | 0.1–0.9, single global value shared by all spotlight regions |

"Sync tool defaults" (default on): color, width, font size, corner radius, watermark opacity/rotation
are shared across compatible tools when nothing is selected. Off = each tool remembers its own.
Editing a selected item's numbers never changes tool defaults. A slider drag is one undo step.

Hold `Shift` while drawing rectangle/filled/oval to lock a square; line and straight arrow snap to
45-degree increments (curved arrows are unconstrained).

### 4.4 Per-tool behavior

- **Arrow**: styles straight / curved-right / curved-left; types classic / tapered / outlined;
  endpoint heads none / arrow / circle at start and end (only classic supports separate heads —
  tapered/outlined bake the head into the body). Endpoints and the curve control point are draggable
  handles. Dashed/dotted applies to classic shafts only; heads stay solid.
- **Text**: presentations plain (transparent), label (filled rounded box), callout (label with a
  draggable tail). Editing commits as a single undo step. Double-click to re-edit.
- **Highlighter**: renders at 3x stroke width with multiply-like blending. Near-straight strokes
  auto-straighten. **Text snapping** (default on): text lines are detected once per image (OCR
  line boxes); a drag across text produces one bar per line, each centered on its line with height
  1.15x the median line height and ends snapped to word boundaries — one undo step for the whole
  sweep. Hold `Ctrl` mid-drag to bypass; falls back to freehand when no text is near, the drag is
  vertical/short, or it leaves the text band.
- **Blur**: effect types pixelated (default), gaussian, hexagonal, crystallized, pointillism,
  halftone, tape, washi. A strength control (1–20) maps to block size / radius (pixel size
  `6 + 2n`, gaussian radius `8 + 4n`). Previews may be cached at reduced quality; export always
  renders at full quality. Blur items always render *below* markup and *above* embedded images.
- **Spotlight**: everything outside the union of spotlight rectangles is dimmed; overlapping regions
  merge. One dim opacity for the whole image.
- **Counter**: numbered circle, auto-increments per placement; diameter `12 + 4n` from the size
  control. Renumbering after deletion is not automatic.
- **Watermark**: text; styles single / diagonal (rotated -24) / tiled (repeated, rotated -24);
  color, opacity, size, rotation. An image watermark is a Omashot extension (drop an image onto the
  watermark tool).
- **Pencil**: freehand path; cannot be resized, only moved.
- **Crop**: handles can shrink *and* expand the canvas (expanding adds blank annotatable area).
  Aspect presets Free / 1:1 / 4:3 / 3:2 / 16:9 / 21:9 with a portrait toggle. In Free mode edges
  snap to detected content borders (default on, `Ctrl` bypasses; no snapping for fixed-aspect or
  Shift-locked drags). `A` or the toolbar button auto-tightens the rect to content; toast if
  nothing found. `Esc` restores the pre-crop rect; `Enter`/`Ctrl+S` commits. A crop toolbar replaces
  the bottom-right controls while cropping.

### 4.5 Selection, transform, editing

- Click selects; marquee selects many; `Shift`-click toggles. Handles: 8 resize handles for
  resizable items (not pencil/highlight), endpoint handles for arrows/lines, tail handle for
  callouts. Rotation is per-item for text/watermark via the rotation control.
- Arrow keys nudge the selection by 1 px, `Shift`+arrow by 10 px.
- `Ctrl+C` / `Ctrl+V` / `Ctrl+D` copy, paste, duplicate annotation items within the session.
  Pastes land centered under the cursor when it is over the canvas; repeated pastes and duplicates
  offset 10 px diagonally. `Delete`/`Backspace` removes the selection.
- `Ctrl+A` selects all. `Esc` deselects, then exits the current tool to Selection.

### 4.6 Undo / redo

`Ctrl+Z` / `Ctrl+Shift+Z`. Undo entries are either an annotation snapshot (items + selection +
embedded-layer metadata so paste/duplicate undo is atomic and redo reselects) or a rotation
snapshot (90-degree canvas rotations have their own entries and never disturb the annotation
stack). Text edits and slider gestures each collapse to one entry.

### 4.7 Zoom / pan

Range 0.25x–4x by default, hard max 16x, max grows for very tall images so 1:1 stays reachable.
`Ctrl+scroll`, touchpad pinch, `Ctrl+=`, `Ctrl+-`, `Ctrl+0` (fit), `Ctrl+1` (actual pixels), zoom
picker presets in the bottom bar. Hold `Space` and drag to pan.

### 4.8 Backgrounds, canvas, mockup

- Sidebar "Canvas": background none / gradient (8 presets: pink-orange, blue-purple, green-blue,
  orange-red, purple-pink, blue-green, yellow-orange, cyan-blue) / wallpaper (bundled set + custom
  file) / blurred copy of the image (soft / frosted / vivid / dim) / solid color; padding
  (default 0); shadow intensity (default 0.3, radius 15, offset y 8); corner radius (default 0);
  aspect ratio Auto / Free / 1:1 / 4:3 / 3:2 / 16:9 with landscape/portrait toggle; 9-way image
  alignment inside the canvas.
- **Canvas presets**: named bundles of the above; one may be marked default (auto-applied to new
  captures, §2.5).
- **Mockup mode** (`M`): 3D tilt with rotation X/Y/Z, perspective, shadow; 8 presets: flat, left
  tilt, right tilt, top view, isometric left, isometric right, hero shot, dramatic. Rendered via a
  GL/skia transform of the flat render.
- Rotate canvas 90 degrees left/right from the toolbar.

### 4.9 Output actions

- `Ctrl+S` save (in place; the file is overwritten and the editable session persisted).
- `Ctrl+Shift+C` copy & close. `Ctrl+C` with nothing selected copies the rendered image.
- Export as... (format picker; PNG/JPG/WebP + quality) to a chosen path.
- **Drag-to-app**: a drag handle in the bottom bar offers a file URI (and image data) of the current
  render; a fallback file is staged under the cache so URI-only targets accept the first drag.
  After a successful drop: `close_after_drag` (default on) saves and closes; otherwise
  `bring_forward_after_drag` (default off) re-raises the editor.
- **Keep editing**: save without closing keeps the window and refreshes the QA card thumbnail.
- Closing with unsaved changes prompts (Save / Discard / Cancel). No draft autosave.

### 4.10 Persisted editable sessions

Every committed save writes a session package next to the history database, keyed by a hash of the
file path: `manifest.json` (schema v1: items, canvas effects, crop, source logical size, a signature
of the file = size + mtime + extension) + `original.bin` (untouched source pixels) + optional
`cutout.png` + `assets/` (embedded images). Re-opening a saved screenshot (from Quick Access,
history, or "open file") restores annotations fully editable on top of the original pixels. If the
file on disk no longer matches the signature, the session is ignored and the flattened image opens.
Sessions are deleted with the file, with history clear, and by the retention sweep. Moving a cache
file to the export folder moves its session.

Opening a file without a session derives its logical size from the DPI metadata.

### 4.11 Automatic sensitive-data redaction

From the blur tool's quick bar (unbound action shortcut by default): OCR the image locally, detect
emails, phone numbers, URLs, credit-card numbers (Luhn + issuer prefixes, including
number/expiry/name rows), `key=value` credentials, and bearer/API tokens (AWS, GitHub, Slack,
Stripe, OpenAI, JWT shapes). Each hit becomes an ordinary editable pixelated blur item, all in one
undo step. Recognized text is never stored.

### 4.12 Extract text

Right-click the image -> Extract Text: runs the configured OCR on the source and copies the result,
reporting through the same notification as OCR capture. Disabled in combine/mockup/preview modes.

### 4.13 Combine images

Entry from the tray menu or `omashot combine a.png b.png` (2+ files; fewer opens a picker). Modes:
auto-stitch (direction smart / horizontal / vertical) or free canvas with edge snapping. The
combine layout is persisted inside the session manifest. "Combine save-as-edit" (default on) saves
the result as an edit of the first image rather than a new file.

### 4.14 Remove background (later)

Non-destructive subject cutout overlay with optional auto-crop; revert restores the original.
Depends on the same segmentation model as object cutout.

### 4.15 Clipboard image on open

When the editor is opened empty and the clipboard holds an image: `ask` (default) /
`load_automatically` / `do_nothing`.

---

## 5. Capture history

- **Storage**: SQLite at `$XDG_DATA_HOME/omashot/omashot.db`. Each record: id, file path, kind
  (screenshot), created-at, pixel width/height, file size, app id, thumbnail path, flags
  (saved vs cache, pinned). Files are *referenced*, not copied. Thumbnails: JPEG, max side 208 px,
  in `thumbnails/`; in-memory LRU ~160 items.
- **Panel**: floating layer-shell window toggled by shortcut (`Super+Shift+H`), tray, or CLI.
  Positions top-center / bottom-center; scale 0.8–1.4; background hud (translucent) / solid.
  Two modes toggled with `Ctrl+E`:
  - *compact*: type filter pills + a horizontal carousel of the newest N cards (N = max items,
    3–20, default 10).
  - *expanded*: filter pills + filename search (150 ms debounce) + time filter all / 24h / 7d / 30d
    + 4-column grid with multi-select and a selection action bar.
- **Keyboard** (when no text field focused): `Ctrl+C` copy selection, `Ctrl+A` select all,
  `Delete` delete, `Enter` open.
- **Card actions**: double-click opens the editor; a "Restore" pill re-creates the Quick Access card
  (deduplicated by path) and opens the editor with the editable session if one exists. Context
  menu: Open in file manager, Copy, Edit, Delete (last).
- **Retention**: sweep at launch and every 24 h. `retention_days` (default 30, 0 = forever),
  `max_count` (default 500, 0 = unlimited). The sweep deletes expired/overflow records, cache
  files no record references, orphan thumbnails, and orphan sessions. "Clear history" removes
  records, thumbnails, and sessions but leaves exported files on disk. Storage size and "open cache
  folder" are shown in preferences.
- With history disabled, nothing is recorded and cache files are cleaned aggressively.

---

## 6. Configuration file

Path: `~/.config/omashot/config.toml` (respect `$XDG_CONFIG_HOME`). Omashot reads it at launch and
watches it with inotify; the app also writes back debounced when preferences change in the UI. If
the file was edited externally since the last write, the UI asks before overwriting. Unknown keys
are ignored; known keys are type/range validated; an invalid file applies nothing and reports the
errors. Secrets (remote OCR API key) are never written here — they go to libsecret.

Keys mirror the reference layout in spirit. Defaults shown.

```toml
schema_version = 1
omashot_min_version = "0.1.0"

[general]
language = "system"              # "system" | BCP-47 tag
appearance = "system"            # "system" | "light" | "dark"
play_sounds = true
url_scheme_enabled = true        # enables the CLI/D-Bus control surface
show_tray_icon = true
start_at_login = false
export_location = "~/Pictures/Screenshots"

[diagnostics]
enabled = true
retention_days = 3               # 1–30

[menu_bar]                       # tray menu customization
icon_style = "default"           # "default" | "viewfinder" | "camera" | "scissors" | "photo" | "custom"
item_order = ["captureArea", "captureAreaAnnotate", "captureApplication", "captureFullscreen",
              "captureActiveWindow", "captureOCR", "openAnnotate", "combineImages",
              "openHistory", "shortcutList"]
hidden_items = []

[capture]
hide_desktop_icons = false

[capture.naming]
screenshot_template = "Omashot_{datetime}_{ms}"

[capture.screenshot]
format = "png"                   # "png" | "jpg" | "webp"
jpeg_quality = 90                # Omashot addition
webp_quality = 80                # Omashot addition
include_own_windows = false
show_cursor = false
freeze_area = false
show_selection_area_overlay = true
reverse_magnifier_zoom_direction = false
show_magnifier_by_default = false
show_magnifier_color_panel = true
remember_last_area = true        # Omashot addition (reference always remembers)

[capture.ocr]
success_notification = true
link_detection = true
selected_model = "builtin"       # "builtin" | "custom:<uuid>"
custom_models = "[]"             # JSON array of {id,name,base_url,model,prompt}; no keys
languages = ["eng"]              # Omashot addition: tesseract packs

[capture.object_cutout]
auto_crop = true

[capture.after.screenshot]
save = true
quick_access = true
copy_file = true
open_annotate = false

[quick_access]
enabled = true
position = "bottomRight"         # "topLeft" | "topRight" | "bottomLeft" | "bottomRight"
auto_dismiss = true
auto_dismiss_delay = 10.0        # 3–30
pause_countdown_on_hover = true
overlay_scale = 1.0              # 0.75–1.5
drag_drop = true
two_finger_swipe_to_dismiss = true
swipe_sensitivity = 1.0          # 0.5–3.0
trackpad_swipe_mode = "inverted" # "natural" | "inverted"
swipe_left_action = "dismiss"
swipe_right_action = "dismiss"
hide_card_when_window_open = true
animation_style = "slide"        # "slide" | "scale"
actions_order = ["copy", "saveOrOpen", "edit", "pinToScreen", "dismiss", "delete"]
enabled_actions = ["copy", "saveOrOpen", "edit", "pinToScreen", "dismiss", "delete"]

[quick_access.slots]
center_top = "copy"
center_bottom = "saveOrOpen"
top_leading = "delete"
top_trailing = "dismiss"
bottom_leading = "edit"
bottom_trailing = ""             # empty = unassigned (reference: upload; omitted here)

[history]
enabled = true
retention_days = 30              # 0 = forever, max 90 in UI
max_count = 500                  # 0 = unlimited, max 1000 in UI
background_style = "hud"         # "hud" | "solid"
open_on_launch = false

[history.floating]
enabled = true
position = "bottomCenter"        # "topCenter" | "bottomCenter"
default_filter = "all"           # "all" | "screenshots"
max_displayed_items = 10         # 3–20
scale = 1.0                      # 0.8–1.4
auto_clear_days = 0              # reserved; unused in reference

[annotate]
clipboard_image_open_behavior = "ask"   # "ask" | "loadAutomatically" | "doNothing"
default_tool = "selection"
remember_last_tool = false
close_after_drag = true
bring_forward_after_drag = false
quick_properties_sync = true
combine_save_as_edit = true
crop_snap_to_edges = true
highlighter_text_snapping = true
```

Shortcut tables (all optional; a missing table means default; `key = ""` clears):

```toml
[shortcuts]
enabled = true

[shortcuts.global.<kind>]        # kinds listed in §7.1
key = "3"
modifiers = ["super", "shift"]   # "super" | "ctrl" | "alt" | "shift"
enabled = true

[shortcuts.overlay.area_application_capture]
key = "a"
modifiers = []                   # no modifiers = in-overlay toggle; with modifiers = its own global hotkey

[shortcuts.annotate_tools.<tool>]   # selection, crop, rectangle, filled_rectangle, oval, arrow,
key = "v"                           # line, text, highlighter, blur, spotlight, counter, watermark, pencil
enabled = true

[shortcuts.annotate_actions.<action>]   # copy_and_close, toggle_sidebar, toggle_pin,
key = "c"                               # auto_redact_sensitive_data
modifiers = ["ctrl", "shift"]
enabled = true

[shortcuts.quick_access.edit_latest_capture]
enabled = false
key = "return"
modifiers = ["ctrl"]

[shortcuts.quick_access.card_actions]
enabled = true

[shortcuts.quick_access.card_actions.<action>]   # copy, save_or_open, dismiss, delete, edit, pin_to_screen
key = "c"
modifiers = ["ctrl"]
enabled = true
```

Validation rules: card-action and annotate-action bindings must include at least one of
ctrl/alt/super (bare and shift-only are rejected); duplicates within a namespace are rejected;
cross-namespace duplicates between global, overlay, annotate-action, and annotate-tool bindings are
rejected; a card-action binding equal to a global shortcut is accepted with a warning. Named keys:
`return`, `delete`, `esc`, `tab`, `space`, `up/down/left/right`, `f1`..`f12`.

Since Hyprland owns global keybinds, Omashot also emits a ready-to-include snippet
(`~/.config/omashot/hyprland.conf`) generated from `[shortcuts.global.*]`, e.g.
`bind = SUPER SHIFT, 3, exec, omashot capture fullscreen`.

---

## 7. Shortcuts

### 7.1 Global (proposed Linux defaults)

| Kind | Action | macOS reference | Omashot default |
| --- | --- | --- | --- |
| `fullscreen` | Capture fullscreen | Cmd+Shift+3 | `Super+Shift+3` |
| `area` | Capture area | Cmd+Shift+4 | `Super+Shift+4` |
| `repeat_area` | Repeat last area | Ctrl+Cmd+Shift+4 | `Super+Ctrl+Shift+4` |
| `area_annotate` | Area + inline annotate | Cmd+Shift+7 | `Super+Shift+7` |
| `active_window` | Capture active window | Cmd+Shift+9 | `Super+Shift+9` |
| `ocr` | Capture text | Cmd+Shift+2 | `Super+Shift+2` |
| `object_cutout` | Object cutout (later) | Cmd+Shift+1 | `Super+Shift+1` (unbound until implemented) |
| `annotate` | Open empty editor | Cmd+Shift+A | `Super+Shift+A` |
| `history` | Toggle history panel | Cmd+Shift+H | `Super+Shift+H` |
| `shortcut_list` | Shortcut cheat sheet | Cmd+Shift+K | `Super+Shift+K` |

Not carried over: recording, scrolling capture, smart element (needs an accessibility tree Wayland
does not expose), video editor, cloud uploads.

Shortcut overlay: a fullscreen cheat sheet listing every binding by group; `Esc` closes; an "Open
settings" button deep-links to the Shortcuts page. Conflict detection warns when a global binding
collides with a known compositor default (e.g. Hyprland's own `Super+Shift+S` screenshot bind in
common configs).

### 7.2 In-overlay (area selection)

| Key | Action |
| --- | --- |
| `A` | Toggle window-pick mode |
| `Esc` / right-click | Cancel |
| `Enter` | Repeat last area (no drag yet) / confirm pending rect |
| `Space` (hold) | Move rectangle while dragging |
| `Shift` (hold) | Lock 1:1 (Omashot addition) |
| Arrows / `Shift`+arrows | Nudge pending rect 1 / 10 px (Omashot addition) |
| Scroll | Magnifier zoom |

Inline annotate adds: `Enter`/`Ctrl+S` finish, `Ctrl+C` copy render, tool keys `V R F O A L T H B S
N W P`.

### 7.3 Editor

| Binding | Action |
| --- | --- |
| `V C R F O A L T H B S N W P M` | Tools (§4.2) |
| `Ctrl+Z` / `Ctrl+Shift+Z` | Undo / redo |
| `Ctrl+C` / `Ctrl+V` / `Ctrl+D` | Copy / paste / duplicate items (image copy when nothing selected) |
| `Ctrl+A` | Select all |
| `Delete` / `Backspace` | Delete selection |
| Arrows / `Shift`+arrows | Nudge 1 / 10 px |
| `Ctrl+S` | Save |
| `Ctrl+Shift+C` | Copy & close |
| `Ctrl+B` | Toggle sidebar |
| `Ctrl+Alt+P` | Toggle pin |
| `Ctrl+=` / `Ctrl+-` / `Ctrl+0` / `Ctrl+1` | Zoom in / out / fit / 1:1 |
| `Space`+drag | Pan |
| `Ctrl`+scroll | Zoom |
| `Esc` | Deselect -> Selection tool -> (crop) cancel crop |
| `Enter` | (crop) commit; (text) newline is `Shift+Enter`, `Enter` commits |
| `A` (in crop) | Auto-crop to content |
| `Shift` while drawing | Square / 45-degree lock |
| `Ctrl` while dragging | Bypass crop edge snap / highlighter text snap |
| unbound | Auto-redact sensitive data |

### 7.4 Quick Access (hover)

`Ctrl+C` copy, `Ctrl+S` save/open, `Ctrl+E` edit, `Ctrl+P` pin, `Ctrl+Backspace` delete,
`Ctrl+W` dismiss; `Ctrl+Enter` edit latest (off by default).

### 7.5 History panel

`Ctrl+E` toggle compact/expanded, `Ctrl+C` copy, `Ctrl+A` select all, `Delete`, `Enter` open.

### 7.6 CLI / D-Bus verbs (replaces the URL scheme)

`omashot capture {fullscreen|area|repeat-area|window|active-window|area-annotate|ocr|object-cutout}`,
`omashot open {annotate|history|combine <files...>}`, `omashot show shortcuts`,
`omashot settings [--page general|capture|annotate|quick-access|history|shortcuts|advanced|about]`.
Disabled when `general.url_scheme_enabled = false`.

---

## 8. Onboarding and preferences

### 8.1 Onboarding (first launch)

1. Welcome + short feature tour (capture, Quick Access, editor).
2. Permissions: on Linux this reduces to checking the portal / screencopy availability, that the
   notification daemon responds, and that the export folder is writable. Show a status row per item
   with a "test" action rather than OS permission toggles.
3. Global shortcuts: show the proposed Hyprland bind snippet, offer to write it to
   `~/.config/hypr/` include or copy to clipboard; detect if binds already exist.
4. Config file: explain `~/.config/omashot/config.toml`, create it with defaults.
5. Done; "Restart onboarding" remains available from General.

### 8.2 Preferences pages

- **General**: start at login (XDG autostart), play sounds, show tray icon, language, appearance,
  export folder, restart onboarding, report issue.
- **Tray menu**: icon style, item visibility + drag reorder, reset.
- **Capture**: panes General (own windows, cursor, overlay dim, magnifier direction/defaults,
  naming template with live preview, after-capture matrix, cutout auto-crop) / Screenshot (freeze
  area, format + quality, default canvas preset) / OCR (provider list with add-custom sheet: name,
  base URL, model, prompt, API key stored in libsecret, test connection; notification toggle; link
  detection; languages).
- **Annotate**: sync tool defaults, combine save-as-edit, crop snap, highlighter text snap,
  clipboard image behavior, close after drag, bring forward after drag.
- **Quick Access**: action customization with a live preview card (drag actions to slots, swipe
  zones, reset), corner position, size, auto-close + delay + pause on hover, hide when editor open,
  animation style, drag & drop, two-finger swipe (mode, sensitivity, per-direction actions).
- **History**: panel enable + position, default filter, background style, size, max items,
  retention days, max count, storage size + open folder, clear history.
- **Shortcuts**: master toggle; grouped recorders with per-row enable and per-section reset:
  Capture, Tools, History, Quick Access, Quick Access card actions, Annotate actions, Annotate
  tool keys; conflict hints; "export Hyprland binds".
- **Advanced**: config file (open, sync now, status), CLI/D-Bus toggle, diagnostics (enable,
  retention, open log folder).
- **About**: version, links.
