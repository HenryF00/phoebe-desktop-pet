import Cocoa

let screen = NSRect(x:0,y:0,width:1728,height:1050)
let canvas = NSSize(width:1440,height:1400)
func placement(_ board: NSRect, in visible: NSRect = screen) -> NSRect {
    lecturePetFrame(board:board,screen:visible,canvas:canvas,bodyHeight:1228,preferredHeight:560)
}
let board = NSRect(x:260,y:240,width:880,height:740)
let roomy = placement(board)
assert(abs(roomy.height / 1400 * 1228 - 330) < 0.01, "Lecture size caps the visible body without changing the desktop preference")
assert(roomy.minX > board.maxX - 100, "Only the pointer/transparent margin overlaps a board with room beside it")
assert(roomy.minY >= board.minY - 200, "Teacher feet stay alongside the reserved footer")
assert(screen.contains(roomy), "Teacher stays inside usable screen bounds")
let moved = placement(board.offsetBy(dx:40,dy:30))
assert(abs(moved.minX - roomy.minX - 40) < 0.01 && abs(moved.minY - roomy.minY - 30) < 0.01, "Teacher follows board movement")
let edge = placement(NSRect(x:180,y:200,width:1500,height:820))
assert(screen.contains(edge), "A board close to the display edge keeps the compact teacher on screen")
assert(edge.height < roomy.height, "Constrained side space selects a smaller lecture body")
let secondary = NSRect(x:-1512,y:70,width:1512,height:912)
let secondaryBoard = NSRect(x:-1390,y:290,width:880,height:660)
assert(secondary.contains(placement(secondaryBoard,in:secondary)), "Negative-origin displays are clamped to their own usable area")
let smaller = lecturePetFrame(board:board,screen:screen,canvas:canvas,bodyHeight:1228,preferredHeight:300)
assert(smaller.height / 1400 * 1228 <= 300.01, "Lecture docking never enlarges a smaller preferred body")
print("PASS: lecture scale, pointer margin, footer clearance, movement, constrained edges and multiple displays")
