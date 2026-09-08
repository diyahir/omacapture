import QtQuick
import Quickshell
import qs.Commons
import qs.Ui
import "Model.js" as Model

// Popout listing every Omashot capture action. Keyboard: Up/Down to move,
// Enter to run, Escape to close.
Panel {
  id: root
  moduleName: "io.github.diyaclanker.omashot"
  manageIpc: false

  property var anchorItem: null
  property var hostWidget: null
  property int cursorIndex: 0
  property bool cursorActive: false

  readonly property var actions: Model.ACTIONS
  readonly property color foreground: bar ? bar.foreground : Color.foreground
  readonly property string fontFamily: bar ? bar.fontFamily : Style.font.family

  function open() {
    root.cursorIndex = 0
    root.cursorActive = false
    root.controller.show()
  }

  function close() {
    root.controller.hide()
  }

  function switchPanel(direction) {
    if (root.bar && typeof root.bar.switchPanelFrom === "function")
      return root.bar.switchPanelFrom(root.hostWidget || root, direction)
    return false
  }

  function run(index) {
    var action = root.actions[Math.max(0, Math.min(index, root.actions.length - 1))]
    root.close()
    Util.execArgv(["omarchy-shell", "omashot", action.id])
  }

  function moveCursor(delta) {
    root.cursorActive = true
    var count = root.actions.length
    root.cursorIndex = (root.cursorIndex + delta + count) % count
  }

  KeyboardPanel {
    id: panel
    anchorItem: root.anchorItem
    owner: root.hostWidget || root
    bar: root.bar
    open: root.opened
    focusTarget: keyCatcher
    contentWidth: panel.fittedContentWidth(Style.space(260))
    contentHeight: panel.fittedContentHeight(content.implicitHeight)

    PanelKeyCatcher {
      id: keyCatcher
      anchors.fill: parent
      onCloseRequested: root.close()
      onTabRequested: function(direction) { root.switchPanel(direction) }
      onMoveRequested: function(dx, dy) { if (dy !== 0) root.moveCursor(dy > 0 ? 1 : -1) }
      onActivateRequested: root.run(root.cursorIndex)
      onReturnRequested: root.run(root.cursorIndex)

      Column {
        id: content
        width: parent.width
        spacing: Style.spacing.xs

        PanelHero {
          width: parent.width
          title: "Omashot"
          meta: "Capture and annotate"
          foreground: root.foreground
          fontFamily: root.fontFamily
          iconComponent: Component {
            Text {
              text: "󰄀"
              color: root.foreground
              font.family: root.fontFamily
              font.pixelSize: Style.font.display
            }
          }
        }

        PanelSeparator {
          width: parent.width
        }

        Repeater {
          model: root.actions

          Button {
            required property var modelData
            required property int index

            width: content.width
            leftAlign: true
            bordered: true
            iconText: modelData.icon
            text: modelData.label
            tooltipText: modelData.hint
            hasCursor: root.cursorActive && index === root.cursorIndex
            foreground: root.foreground
            fontFamily: root.fontFamily
            fontSize: Style.font.body
            onHovered: function(isHovered) {
              if (isHovered) {
                root.cursorActive = true
                root.cursorIndex = index
              }
            }
            onClicked: root.run(index)
          }
        }
      }
    }
  }
}
