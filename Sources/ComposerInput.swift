import Cocoa

/// The accessory app has no document menu bar. Route editing shortcuts explicitly.
final class ComposerEditor: NSTextView {
    var clipboard = NSPasteboard.general
    var commandKey: ((NSEvent)->Bool)?
    var selectionChanged: (()->Void)?
    override func keyDown(with event:NSEvent) {
        if commandKey?(event) == true { return };super.keyDown(with:event);selectionChanged?()
    }
    override func mouseDown(with event:NSEvent) { super.mouseDown(with:event);selectionChanged?() }
    var pasteAttachment: (() -> Bool)?
    var contextMenu: (() -> NSMenu)?
    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        if event.modifierFlags.intersection(.deviceIndependentFlagsMask).contains(.command) {
            switch event.charactersIgnoringModifiers?.lowercased() {
            case "v": paste(nil); return true
            case "c": copy(nil); return true
            case "x": cut(nil); return true
            case "a": selectAll(nil); return true
            case "z": if event.modifierFlags.contains(.shift) { undoManager?.redo() } else { undoManager?.undo() }; return true
            default: break
            }
        }
        return super.performKeyEquivalent(with:event)
    }
    override func paste(_ sender: Any?) {
        if pasteAttachment?() == true { return }
        if let text = clipboard.string(forType: .string) {
            insertText(text, replacementRange: selectedRange())
        } else { NSSound.beep() }
    }
    override func menu(for event: NSEvent) -> NSMenu? { contextMenu?() ?? super.menu(for:event) }
}
