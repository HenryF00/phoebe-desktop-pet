import Cocoa

let app = NSApplication.shared
app.setActivationPolicy(.accessory)
let visible = NSScreen.main!.visibleFrame
let controller = PetController()
controller.manifest = AnimationManifest(width:1440,height:1400,bodyHeight:1228,
    clips:["idle":Clip(label:"待机",frames:["idle"],durations:[1])])
controller.images = ["idle":[NSImage(size:NSSize(width:1440,height:1400))]]
let desktop = NSRect(x:visible.minX+30,y:visible.minY+30,width:320,height:312)
controller.pet = PetView(frame:NSRect(origin:.zero,size:desktop.size))
controller.panel = PetPanel(contentRect:desktop,styleMask:[.borderless,.nonactivatingPanel],backing:.buffered,defer:false)
controller.panel.contentView = controller.pet
let board = NSWindow(contentRect:NSRect(x:visible.minX+100,y:visible.minY+220,width:700,height:500),
                     styleMask:[.titled],backing:.buffered,defer:false)
let preference = controller.settings.object(forKey:"displayHeight") as? Double
controller.placeForLecture(board)
assert(controller.desktopFrameBeforeLecture == desktop, "Docking saves the complete original desktop frame")
assert(!controller.pet.allowsDragging && controller.panel.parent == board, "Teacher belongs to board while docked")
assert(controller.height == 560 && (controller.settings.object(forKey:"displayHeight") as? Double) == preference,
       "Temporary lecture scale never writes the user’s display height preference")
board.setFrameOrigin(board.frame.origin.applying(CGAffineTransform(translationX:20,y:20)))
controller.placeForLecture(board)
assert(controller.desktopFrameBeforeLecture == desktop, "Relayout never replaces the saved desktop frame")
controller.lastCodexPoll = ProcessInfo.processInfo.systemUptime + 100
controller.codexState = "running";controller.automatic = true
controller.tick()
assert(controller.state == "idle", "Task following cannot replace the docked classroom pose")
controller.placeForLecture(nil)
assert(controller.panel.frame == desktop && controller.desktopFrameBeforeLecture == nil, "Closing restores original desktop size and location")
assert(controller.pet.allowsDragging && controller.panel.parent == nil, "Closing restores normal draggable desktop ownership")
controller.placeForLecture(nil)
assert(controller.panel.frame == desktop, "Repeated close notifications do not move the pet")
print("PASS: lecture docking, original frame retention, preference preservation, task suppression and restoration")
