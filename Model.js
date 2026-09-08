.pragma library

// Capture actions exposed by the bar panel and the IPC target. Every entry
// maps to an `omashot` subcommand; the shell only ever launches the binary.
var BINARY = "omashot"

// Where `cargo install` and manual installs put the binary. omarchy-shell's
// own PATH is the login PATH, which does not include ~/.cargo/bin, so the
// service resolves an absolute path instead of relying on PATH.
var CANDIDATES = ["$HOME/.cargo/bin/omashot", "$HOME/.local/bin/omashot", "/usr/local/bin/omashot", "/usr/bin/omashot"]

var ACTIONS = [
  { id: "area",     icon: "󰩬", label: "Capture area",          hint: "Drag a region, A for window mode", args: ["area"] },
  { id: "window",   icon: "󰖯", label: "Capture window",        hint: "Click a window",                   args: ["window"] },
  { id: "full",     icon: "󰍹", label: "Capture screen",        hint: "Every monitor",                    args: ["full"] },
  { id: "annotate", icon: "󰏫", label: "Capture and annotate",  hint: "Open the editor before saving",    args: ["area", "--annotate"] },
  { id: "ocr",      icon: "󰴑", label: "Extract text",          hint: "OCR a region to the clipboard",    args: ["ocr"] },
  { id: "history",  icon: "󰋚", label: "History",               hint: "Browse recent captures",           args: ["history"] },
  { id: "settings", icon: "",  label: "Preferences",           hint: "Omashot settings",                 args: ["settings"] }
]

function actionById(id) {
  for (var i = 0; i < ACTIONS.length; i++) {
    if (ACTIONS[i].id === id) return ACTIONS[i]
  }
  return ACTIONS[0]
}

// argv for the resolved binary; never a shell string.
function argvFor(binary, id) {
  return [binary].concat(actionById(id).args)
}

// Shell snippet that prints the first usable binary path, or fails.
function probeScript() {
  var checks = CANDIDATES.map(function(c) { return '[ -x "' + c + '" ] && { printf "%s\\n" "' + c + '"; exit 0; }' })
  return checks.join("; ") + '; command -v omashot 2>/dev/null && exit 0; exit 1'
}
