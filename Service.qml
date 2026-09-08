import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import "Model.js" as Model

// Headless service: resolves where the omashot binary lives (cargo install
// puts it in ~/.cargo/bin, which is not on omarchy-shell's PATH), keeps the
// daemon resident so the capture hotkey responds instantly, and exposes an
// IPC target so keybindings, the bar widget, and scripts trigger captures
// through the shell:
//   omarchy-shell omashot area | window | full | annotate | ocr | history | settings
Item {
  id: root

  // Injected by omarchy-shell.
  property var shell: null

  property bool installed: false
  property bool checked: false
  // Absolute path once the probe has found it; falls back to PATH lookup.
  property string binary: Model.BINARY

  function capture(mode) {
    // Only report "missing" once the probe has actually run and failed;
    // right after a plugin reload the probe may still be in flight.
    if (root.checked && !root.installed) {
      Util.execArgv(["omarchy-notification-send", "Omashot is not installed", "Build the omashot binary with the step from the plugin README, then run: omarchy-shell omashot recheck"])
      return "missing"
    }
    Util.execArgv(Model.argvFor(root.binary, mode))
    return "ok"
  }

  function ensureDaemon() {
    if (!root.installed || daemon.running) return
    daemon.command = [root.binary, "daemon"]
    daemon.running = true
  }

  Process {
    id: probe
    command: ["bash", "-c", Model.probeScript()]
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        var found = String(text || "").trim().split("\n")[0]
        if (found !== "") root.binary = found
      }
    }
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
      return JSON.stringify({ installed: root.installed, checked: root.checked, daemon: daemon.running, binary: root.binary })
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
      // argv form: the path never passes through a shell.
      Util.execArgv([root.binary, "annotate", "--", path])
      return "ok"
    }

    function recheck(): void {
      root.checked = false
      probe.running = true
    }
  }
}
