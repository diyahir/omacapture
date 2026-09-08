import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import "Model.js" as Model

// Headless service: keeps the omashot daemon resident so the capture hotkey
// responds instantly, and exposes an IPC target so keybindings and scripts
// can trigger captures through the shell:
//   omarchy-shell omashot area | window | full | annotate | ocr | history
Item {
  id: root

  // Injected by omarchy-shell.
  property var shell: null

  property bool installed: false
  property bool checked: false

  function capture(mode) {
    // Only report "missing" once the probe has actually run and failed;
    // right after a plugin reload the probe may still be in flight.
    if (root.checked && !root.installed) {
      Util.execDetached("omarchy-notification-send 'Omashot is not installed' 'Run: cargo install --path ~/.config/omarchy/plugins/io.github.diyaclanker.omashot'")
      return "missing"
    }
    Util.execDetached(Model.commandFor(mode))
    return "ok"
  }

  function ensureDaemon() {
    if (!root.installed || daemon.running) return
    daemon.running = true
  }

  Process {
    id: probe
    command: ["bash", "-lc", "command -v " + Model.BINARY]
    onExited: function(exitCode) {
      root.installed = exitCode === 0
      root.checked = true
      if (root.installed) root.ensureDaemon()
    }
  }

  // The daemon is a single-instance GTK app; if one already runs elsewhere
  // this child forwards to it and exits, which is harmless.
  Process {
    id: daemon
    command: [Model.BINARY, "daemon"]
  }

  Component.onCompleted: probe.running = true

  IpcHandler {
    target: "omashot"

    function status(): string {
      return JSON.stringify({ installed: root.installed, checked: root.checked, daemon: daemon.running })
    }

    function area(): string { return root.capture("area") }
    function window(): string { return root.capture("window") }
    function full(): string { return root.capture("full") }
    function annotate(): string { return root.capture("annotate") }
    function ocr(): string { return root.capture("ocr") }
    function history(): string { return root.capture("history") }
    function settings(): string { return root.capture("settings") }

    function edit(path: string): string {
      if (root.checked && !root.installed) return "missing"
      Util.execDetached(Model.editCommand(path))
      return "ok"
    }

    function recheck(): void {
      probe.running = true
    }
  }
}
