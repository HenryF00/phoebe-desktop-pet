import Cocoa
let app=NSApplication.shared;app.setActivationPolicy(.accessory)
let root=URL(fileURLWithPath:CommandLine.arguments[1])
let qa=root.appendingPathComponent("qa/lesson")
try FileManager.default.createDirectory(at:qa,withIntermediateDirectories:true)
let controller=TeachingBoardController()
var submissions:[String]=[],layouts=0,began=0,ended=0
controller.onFollowUp={submissions.append($0)};controller.onLayout={layouts += 1}
controller.onRecordBegin={began += 1};controller.onRecordEnd={ended += 1}
controller.show(["task_id":"lesson-surface","title":"和洛琪希一起看懂彩虹","takeaway":"光进入雨滴，颜色就慢慢分开了。","points":[["heading":"雨滴像小小的棱镜","explanation":"阳光并不是单一颜色。不同颜色的光经过水滴时，弯曲的角度不同，我们便看见了彩虹。"]],"closing_caption":"这部分就讲到这里啦。还有哪里想一起看看？"])
let footer=controller.captions
assert(footer.panel.isVisible && footer.label.isHidden,"A silent board must keep its input visible")
assert(footer.panel.canBecomeKey && footer.input.placeholderString==nil && !footer.input.isBordered)
assert(BoardTypography.font(18).fontName==BoardTypography.fontName,"The bundled handwriting must really load")
let contentFont=controller.document.textStorage!.attribute(.font,at:0,effectiveRange:nil) as! NSFont
assert(contentFont.fontName==BoardTypography.fontName)
footer.panel.makeKeyAndOrderFront(nil);footer.panel.makeFirstResponder(footer.input)
RunLoop.main.run(until:Date().addingTimeInterval(0.08))
let editor=footer.input.currentEditor() as! NSTextView
editor.string="为什么有时候会有两道彩虹？"
let event=NSEvent.keyEvent(with:.keyDown,location:.zero,modifierFlags:[],timestamp:0,windowNumber:footer.panel.windowNumber,context:nil,characters:"\r",charactersIgnoringModifiers:"\r",isARepeat:false,keyCode:36)!
editor.keyDown(with:event)
assert(submissions==["为什么有时候会有两道彩虹？"],"Return sends exactly once")
assert(footer.userText.stringValue==submissions[0] && footer.input.stringValue.isEmpty)
footer.input.stringValue="不要重复发送";footer.setInputEnabled(false);footer.submit()
assert(submissions.count==1 && !footer.mic.isEnabled)
footer.setInputEnabled(true);footer.input.stringValue=""
footer.mic.pressed?();footer.mic.released?();assert(began==1 && ended==1)
controller.beginSegment(LessonAudio(data:Data(),caption:"好问题！第二道彩虹，来自阳光在雨滴里多转了一次弯。",focus:"point-0",gesture:"point",index:0))
assert(!footer.label.isHidden && footer.panel.isVisible)
controller.endLesson(completed:false)
assert(!footer.label.stringValue.contains("这部分就讲到这里"),"Cancellation must not show a completion")
controller.endLesson(completed:true)
assert(footer.label.stringValue.contains("还有哪里"))
assert(layouts>0 && footer.panel.frame.maxY<controller.window.frame.minY)
controller.window.setFrameOrigin(NSScreen.main!.visibleFrame.origin)
footer.layout(below:controller.window)
assert(NSScreen.main!.visibleFrame.contains(footer.panel.frame))
func capture(_ window:NSWindow,_ filename:String) throws {
    let view=window.contentView!;view.layoutSubtreeIfNeeded();RunLoop.main.run(until:Date().addingTimeInterval(0.08))
    let bitmap=view.bitmapImageRepForCachingDisplay(in:view.bounds)!
    window.appearance!.performAsCurrentDrawingAppearance { view.cacheDisplay(in:view.bounds,to:bitmap) }
    try bitmap.representation(using:.png,properties:[:])!.write(to:qa.appendingPathComponent(filename))
}
try capture(controller.window,"lecture-handwriting.png")
try capture(footer.panel,"lecture-conversation-footer.png")
controller.window.close();assert(!footer.panel.isVisible)
print("PASS: persistent lesson footer, exact-once Return, hold mic, handwriting, completion-only invitation, layout and close")
