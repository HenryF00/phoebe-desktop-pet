import Cocoa

// The transparent canvas includes room for the left-facing teaching pointer.
// Keep the visible body beside the board and leave its lower edge alongside the footer.
func lecturePetFrame(board: NSRect, screen: NSRect, canvas: NSSize,
                     bodyHeight: CGFloat, preferredHeight: CGFloat,
                     footerHeight: CGFloat = 200) -> NSRect {
    guard bodyHeight > 0, canvas.width > 0, canvas.height > 0 else { return board }
    let widthRatio = canvas.width / bodyHeight
    let heightRatio = canvas.height / bodyHeight
    let rightRoom = max(0, screen.maxX - board.maxX - 8)
    let desiredHeight = min(preferredHeight, 330, screen.height * 0.46)
    // A compact teacher is still legible when the board reaches the screen edge.
    let compactHeight = min(desiredHeight, 220)
    let displayedHeight = max(compactHeight, min(desiredHeight, rightRoom / (widthRatio * 0.76)))
    let size = NSSize(width: displayedHeight * widthRatio, height: displayedHeight * heightRatio)
    let belowBoard = min(max(0, footerHeight - 20), displayedHeight * 0.48)
    var result = NSRect(x: board.maxX - size.width * 0.24,
                        y: board.minY - belowBoard - size.height * 0.08,
                        width: size.width, height: size.height)
    result.origin.x = min(max(result.minX, screen.minX), max(screen.minX, screen.maxX - result.width))
    result.origin.y = min(max(result.minY, screen.minY), max(screen.minY, screen.maxY - result.height))
    return result
}
