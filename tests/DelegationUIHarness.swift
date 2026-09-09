import Cocoa
let app=NSApplication.shared;app.setActivationPolicy(.accessory)
let root=URL(fileURLWithPath:CommandLine.arguments[1])
let voice=VoiceController(root:root,startWorker:false)
voice.show()
assert(voice.window.firstResponder === voice.composerEditor, "Composer must use the custom field editor")
let board=NSPasteboard(name:NSPasteboard.Name("roxy-private-test"))
voice.composerEditor.clipboard=board
board.clearContents();board.setString("委托：分析这段内容\n第二行",forType:.string)
let paste=NSEvent.keyEvent(with:.keyDown,location:.zero,modifierFlags:.command,timestamp:0,windowNumber:voice.window.windowNumber,context:nil,characters:"v",charactersIgnoringModifiers:"v",isARepeat:false,keyCode:9)!
assert(voice.window.performKeyEquivalent(with:paste))
assert(voice.input.stringValue.contains("第二行"))
board.clearContents();board.writeObjects([NSImage(contentsOf:root.appendingPathComponent("assets/pet/frames/master.png"))!])
assert(voice.window.performKeyEquivalent(with:paste))
assert(voice.attachmentPaths.count==1 && voice.taskMode)
let attachment=voice.attachmentPaths[0];assert(FileManager.default.fileExists(atPath:attachment))
voice.clearAttachments();assert(!FileManager.default.fileExists(atPath:attachment))
let select=NSEvent.keyEvent(with:.keyDown,location:.zero,modifierFlags:.command,timestamp:0,windowNumber:voice.window.windowNumber,context:nil,characters:"a",charactersIgnoringModifiers:"a",isARepeat:false,keyCode:0)!
assert(voice.window.performKeyEquivalent(with:select))
assert(voice.composerEditor.selectedRange().length > 0)
voice.receive(["type":"projects","projects":[["id":"project","name":"Demo","path":root.path]]])
let menu=voice.projectMenu()
let project=menu.items.first{$0.title=="Demo"}!
voice.selectProject(project)
assert(voice.taskMode && voice.selectedProject?["id"] == "project")
assert(voice.input.placeholderString==nil && voice.input.toolTip?.contains(root.path)==true)
let ui=DelegatedTasksController()
let job:[String:Any]=["id":"test","title":"核对项目构建结果","prompt":"运行构建并说明结果","cwd":root.path,"status":"completed","result":"构建通过，输出文件已经生成。","created":1.0]
ui.update([job]);ui.show()
assert(ui.detail.string.contains("构建通过"))
var response:[String:Any]?
ui.command={response=$0}
var waiting=job
waiting["status"]="waiting"
waiting["request"]=["token":"exact-request","method":"item/commandExecution/requestApproval","details":["command":"npm test","cwd":root.path,"reason":"运行项目测试"]]
ui.update([waiting]);ui.acceptRequest()
assert(response?["token"] as? String == "exact-request" && response?["accepted"] as? Bool == true)
ui.declineRequest();assert(response?["accepted"] as? Bool == false)
let qa=root.appendingPathComponent("qa/delegation-ui")
try FileManager.default.createDirectory(at:qa,withIntermediateDirectories:true)
for mode in [NSAppearance.Name.aqua,.darkAqua] {
    ui.window.appearance=NSAppearance(named:mode);let view=ui.window.contentView!;view.layoutSubtreeIfNeeded()
    ui.table.reloadData();RunLoop.main.run(until:Date().addingTimeInterval(0.15))
    let bitmap=view.bitmapImageRepForCachingDisplay(in:view.bounds)!
    ui.window.appearance!.performAsCurrentDrawingAppearance{view.displayIfNeeded();view.cacheDisplay(in:view.bounds,to:bitmap)}
    try bitmap.representation(using:.png,properties:[:])!.write(to:qa.appendingPathComponent("tasks-\(mode.rawValue).png"))
}
ui.window.orderOut(nil);voice.shutdown()
print("PASS: composer field editor, Cmd-V text and image paste, private clipboard, attachment removal, Cmd-A dispatch, project menu, context indication, task result, exact approval/denial")
