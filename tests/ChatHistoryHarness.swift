import Cocoa
let app=NSApplication.shared;app.setActivationPolicy(.accessory)
let realRoot=URL(fileURLWithPath:CommandLine.arguments[1])
let temp=FileManager.default.temporaryDirectory.appendingPathComponent("roxy-history-"+UUID().uuidString)
try FileManager.default.createDirectory(at:temp,withIntermediateDirectories:true)
defer { try? FileManager.default.removeItem(at:temp) }
let voice=VoiceController(root:temp,startWorker:false,persistHistory:true)
voice.setDisplayedUser("这是一条打字发出的消息")
assert(voice.userCaption.stringValue == "这是一条打字发出的消息" && !voice.userSection.isHidden)
voice.currentID="asr";voice.generating=true
voice.receive(["type":"user","id":"asr","text":"语音识别出来的中文内容"])
assert(voice.userCaption.stringValue == "语音识别出来的中文内容")
voice.receive(["type":"delta","id":"asr","text":"我听见了，先把今天的事情慢慢说给我听吧。"])
voice.receive(["type":"done","id":"asr","text":"我听见了，先把今天的事情慢慢说给我听吧。"])
assert(voice.userCaption.stringValue == "语音识别出来的中文内容")
assert(voice.transcript.string.contains("语音识别出来的中文内容"))
assert(FileManager.default.fileExists(atPath:voice.historyURL.path))
let attrs=try FileManager.default.attributesOfItem(atPath:voice.historyURL.path)
assert((attrs[.posixPermissions] as? NSNumber)?.intValue == 0o600)
let reopened=VoiceController(root:temp,startWorker:false,persistHistory:true)
assert(reopened.transcript.string.contains("语音识别出来的中文内容") && reopened.transcript.string.contains("我听见了"))
voice.clearChat()
assert(voice.history.isEmpty && voice.userSection.isHidden && voice.transcript.string.contains("语音识别出来的中文内容"))
voice.setDisplayedUser("语音识别出来的中文内容")
voice.showSubtitle("我听见了，先把今天的事情慢慢说给我听吧。",reveal:false)
assert(voice.subtitle.onDoubleClick != nil && voice.userCaption.onDoubleClick != nil && voice.bubbleGlass.onDoubleClick != nil)
let field=HistoryTextField(labelWithString:"记录");var clicks=0;field.onDoubleClick={clicks += 1}
let event=NSEvent.mouseEvent(with:.leftMouseDown,location:.zero,modifierFlags:[],timestamp:0,windowNumber:0,context:nil,eventNumber:1,clickCount:2,pressure:1)!
field.mouseDown(with:event);assert(clicks==1)
let ui=ChatHistoryController(transcript:voice.transcript)
assert(!voice.transcript.isEditable && voice.transcript.isSelectable && voice.transcript.enclosingScrollView != nil)
let out=realRoot.appendingPathComponent("qa/chat-history-ui");try FileManager.default.createDirectory(at:out,withIntermediateDirectories:true)
voice.renderQA(out.path)
for mode in [NSAppearance.Name.aqua,.darkAqua] {
 ui.window.appearance=NSAppearance(named:mode);ui.window.contentView!.layoutSubtreeIfNeeded()
 let view=ui.window.contentView!;let bitmap=view.bitmapImageRepForCachingDisplay(in:view.bounds)!
 ui.window.appearance!.performAsCurrentDrawingAppearance { view.displayIfNeeded();view.cacheDisplay(in:view.bounds,to:bitmap) }
 try bitmap.representation(using:.png,properties:[:])!.write(to:out.appendingPathComponent("history-\(mode.rawValue).png"))
}
voice.setDisplayedUser(String(repeating:"较长的语音识别内容，需要保留在记录里。",count:15))
voice.showSubtitle(String(repeating:"较长的回复在完整记录里可以查看。",count:10),reveal:false)
voice.bubble.contentView?.layoutSubtreeIfNeeded()
assert(voice.bubble.frame.height < 500)
voice.renderQA(out.appendingPathComponent("long").path)
voice.shutdown();reopened.shutdown()
print("PASS: typed/ASR message retained; local history roundtrip; new conversation preserves archive; double click; selectable history UI")
