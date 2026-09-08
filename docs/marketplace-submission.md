### Repository URL
https://github.com/diyahir/omacapture

### Category
Productivity

### Tags
hyprland, quickshell, system

### Suggest a missing tag
screenshots

### Maintainer notes
The shell plugin (bar widget + service) only launches a native `omacapture` binary that the user builds from the same repository with `cargo install --path` (no remote git, no bundled binaries, no sudo). Runtime dependencies are documented in the README: gtk4, libadwaita, gtk4-layer-shell, grim, wl-clipboard; tesseract is optional for OCR. Everything runs locally; there is no network access. The plugin never edits Hyprland or shell configuration; keybinding and menu snippets are documented for the user to add.

### Submission checklist

- [x] The repository is public and contains installation and removal instructions.
- [x] I have documented the plugin license and any external dependencies.
- [x] I confirm that I own or have permission to submit this plugin and its preview assets.
- [x] The plugin does not overwrite user configuration without explicit consent.
- [x] I understand that approval is for listing and is not a security review.
