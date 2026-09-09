import Cocoa

final class CompanionPanel: NSPanel {
    var acceptsKeyboard = true
    override var canBecomeKey: Bool { acceptsKeyboard }
    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        if let editor=firstResponder as? ComposerEditor, event.modifierFlags.contains(.command), editor.performKeyEquivalent(with:event) { return true }
        return super.performKeyEquivalent(with:event)
    }
    override var canBecomeMain: Bool { false }
}

final class CompanionGlass: NSView {
    var onDoubleClick: (() -> Void)?
    override func mouseDown(with event: NSEvent) {
        if event.clickCount == 2 { onDoubleClick?() } else { super.mouseDown(with:event) }
    }
    var pointsRight = true { didSet { needsDisplay = true } }
    var tail = false
    override func draw(_ dirtyRect: NSRect) {
        NSColor.clear.setFill(); bounds.fill(using: .copy)
        let rect = bounds.insetBy(dx: 8, dy: 1)
        let shape = NSBezierPath(roundedRect: rect, xRadius: 18, yRadius: 18)
        if tail {
            let x = pointsRight ? rect.maxX - 1 : rect.minX + 1
            shape.move(to: NSPoint(x: x, y: rect.midY - 8))
            shape.line(to: NSPoint(x: pointsRight ? bounds.maxX : bounds.minX, y: rect.midY))
            shape.line(to: NSPoint(x: x, y: rect.midY + 8)); shape.close()
        }
        // A translucent backing maintains legibility over both light and dark desktops.
        NSColor.windowBackgroundColor.withAlphaComponent(0.84).setFill(); shape.fill()
        NSColor.labelColor.withAlphaComponent(0.12).setStroke(); shape.lineWidth = 0.7; shape.stroke()
    }
}

/// Screen-clamped geometry, shared by the live anchor and edge-case tests.
func companionFrame(anchor: NSRect, size: NSSize, screen: NSRect) -> (NSRect, Bool) {
    let leftRoom = anchor.minX - screen.minX
    let rightRoom = screen.maxX - anchor.maxX
    let onLeft = leftRoom >= size.width + 12 || leftRoom > rightRoom
    let desiredX = onLeft ? anchor.minX - size.width - 12 : anchor.maxX + 12
    let x = min(max(desiredX, screen.minX + 8), screen.maxX - size.width - 8)
    let y = min(max(anchor.midY - 70, screen.minY + 8), screen.maxY - size.height - 8)
    return (NSRect(x: x, y: y, width: size.width, height: size.height), onLeft)
}
