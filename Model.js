.pragma library

// Capture actions exposed by the bar panel and the IPC target. Every entry
// maps to an `omashot` subcommand; the shell only ever launches the binary.
var BINARY = "omashot"

var ACTIONS = [
  { id: "area",     icon: "󰩬", label: "Capture area",          hint: "Drag a region, A for window mode", command: BINARY + " area" },
  { id: "window",   icon: "󰖯", label: "Capture window",        hint: "Click a window",                   command: BINARY + " window" },
  { id: "full",     icon: "󰍹", label: "Capture screen",        hint: "Every monitor",                    command: BINARY + " full" },
  { id: "annotate", icon: "󰏫", label: "Capture and annotate",  hint: "Open the editor before saving",    command: BINARY + " area --annotate" },
  { id: "ocr",      icon: "󰴑", label: "Extract text",          hint: "OCR a region to the clipboard",    command: BINARY + " ocr" },
  { id: "history",  icon: "󰋚", label: "History",               hint: "Browse recent captures",           command: BINARY + " history" },
  { id: "settings", icon: "",  label: "Preferences",           hint: "Omashot settings",                 command: BINARY + " settings" }
]

function actionById(id) {
  for (var i = 0; i < ACTIONS.length; i++) {
    if (ACTIONS[i].id === id) return ACTIONS[i]
  }
  return ACTIONS[0]
}

function commandFor(id) {
  return actionById(id).command
}
