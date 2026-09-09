import Cocoa
final class SubmissionProbe: NSObject {
    var count=0
    @objc func submit(_ sender:Any?) { count += 1 }
}
let app=NSApplication.shared;app.setActivationPolicy(.accessory)
let root=URL(fileURLWithPath:CommandLine.arguments[1]), qa=root.appendingPathComponent("qa/lesson")
try FileManager.default.createDirectory(at:qa,withIntermediateDirectories:true)
let voice=VoiceController(root:root,startWorker:false)
voice.preferences.subtitleLanguage="zh";voice.preferences.voiceLanguage="ja";voice.preferences.boardSpeech=false
voice.show()
voice.projects=[["id":"hipa","name":"HiPA","path":"/example/HiPA"],["id":"spaces","name":"My Project","path":"/example/My Project"]]
func enter(_ text:String) {
    voice.composerEditor.insertText(text,replacementRange:NSRange(location:0,length:(voice.composerEditor.string as NSString).length))
    voice.updateSuggestions()
}
func key(_ code:UInt16,_ char:String) {
    let event=NSEvent.keyEvent(with:.keyDown,location:.zero,modifierFlags:[],timestamp:0,windowNumber:voice.window.windowNumber,context:nil,characters:char,charactersIgnoringModifiers:char,isARepeat:false,keyCode:code)!
    voice.composerEditor.keyDown(with:event)
}
func capture(_ window:NSWindow,_ name:String) throws {
    let view=window.contentView!;view.layoutSubtreeIfNeeded();RunLoop.main.run(until:Date().addingTimeInterval(0.1))
    let bitmap=view.bitmapImageRepForCachingDisplay(in:view.bounds)!
    (window.appearance ?? NSAppearance.currentDrawing()).performAsCurrentDrawingAppearance { view.cacheDisplay(in:view.bounds,to:bitmap) }
    try bitmap.representation(using:.png,properties:[:])!.write(to:qa.appendingPathComponent(name+".png"))
}
voice.projectsLoaded=true
enter("/");assert(voice.suggestions.choices.count==3 && voice.suggestions.panel.isVisible)
try capture(voice.suggestions.panel,"mode-completion")
voice.suggestions.panel.appearance=NSAppearance(named:.darkAqua)
try capture(voice.suggestions.panel,"mode-completion-dark")
voice.suggestions.panel.appearance=nil
key(125,"\u{F701}");key(48,"\t")
assert(voice.input.stringValue=="/小黑板 " && !voice.suggestions.panel.isVisible && voice.worker==nil)
enter("/委托 @Hi");assert(voice.suggestions.choices.first?.title=="HiPA")
try capture(voice.suggestions.panel,"project-completion")
assert(voice.suggestions.table.rowView(atRow:0,makeIfNecessary:true)!.frame.width <= voice.suggestions.scroll.contentSize.width+1,"Selection corners must not be clipped by system scrollbars")
voice.suggestions.panel.appearance=NSAppearance(named:.darkAqua)
try capture(voice.suggestions.panel,"project-completion-dark")
voice.suggestions.panel.appearance=nil
key(36,"\r");assert(voice.input.stringValue=="/委托 @HiPA " && voice.worker==nil)
let selected=ComposerSubmission.parse("/委托 @\"My Project\" /小黑板 分析报告",projects:voice.projects,selected:nil,taskMode:false)
assert(selected.project?["id"]=="spaces" && selected.mode=="task" && selected.text=="/委托 /小黑板 分析报告")
let chat=ComposerSubmission.parse("/聊天 你好",projects:voice.projects,selected:voice.projects[0],taskMode:true)
assert(chat.project==nil && chat.mode=="chat")
assert(ComposerQuery.at("user@example.com",cursor:16)==nil)
assert(ComposerQuery.at("解释🙂 /小",cursor:("解释🙂 /小" as NSString).length)?.filter=="小")
enter("@Hi");key(53,"\u{1b}");assert(!voice.suggestions.panel.isVisible)
enter("@Unknown");key(48,"\t");assert(voice.input.stringValue=="@Unknown" && voice.worker==nil)
voice.projects=[];voice.projectsLoaded=false;enter("@Hi");key(48,"\t")
assert(voice.input.stringValue=="@Hi" && voice.worker==nil && voice.suggestions.choices.first?.title=="项目正在加载…")
let probe=SubmissionProbe();voice.input.target=probe;voice.input.action=#selector(SubmissionProbe.submit(_:))
enter("普通正文");key(48,"\t");assert(probe.count==0)
key(36,"\r");assert(probe.count==1,"Return must still send; Tab and focus loss must not send")
voice.hideChat()
var board=try JSONSerialization.jsonObject(with:Data(contentsOf:root.appendingPathComponent("qa/blackboard/model.json"))) as! [String:Any]
voice.ensureBoardWindow();voice.boardController!.show(board)
voice.currentAnswer="聊天字幕不应被授课逐段字幕覆盖。";voice.renderReplyText();let oldCaption=voice.subtitle.stringValue
voice.generating=true;voice.boardSpeechRequest="lesson-test"
let data=try Data(contentsOf:root.appendingPathComponent("runtime/lesson-qa-silent.wav"))
voice.receive(["type":"board_audio","request_id":"lesson-test","index":0,"subtitle":"第一段：先看这项数字。","focus":"metric-0","gesture":"point","data":data.base64EncodedString()])
assert(voice.player==nil && voice.boardController!.activeFocus==nil)
voice.generating=false;voice.playNext()
assert(voice.activeBoardSpeech && voice.boardController!.activeFocus=="metric-0")
let first=voice.boardController!.lessonCaption.stringValue
let captionWindow=voice.boardController!.captions.panel
assert(captionWindow.isVisible && captionWindow.frame.maxY < voice.boardController!.window.frame.minY)
assert(voice.input.placeholderString==nil)
try capture(captionWindow,"subtitle-below-board")
assert(voice.boardController!.lessonCaption.window !== voice.boardController!.window,
       "授课字幕必须独立显示在黑板下方，不能混在板书滚动区域内")
voice.receive(["type":"board_audio","request_id":"lesson-test","index":1,"subtitle":"第二段：这里是重点。","focus":"point-1","gesture":"emphasize","data":data.base64EncodedString()])
voice.receive(["type":"board_audio_done","request_id":"lesson-test"])
assert(voice.boardController!.lessonCaption.stringValue==first && voice.boardSpeechRequest != nil)
try capture(voice.boardController!.window,"highlight-metric")
var sawSecond=false;let end=Date().addingTimeInterval(4)
while Date()<end && voice.boardSpeechRequest != nil {
    RunLoop.main.run(until:Date().addingTimeInterval(0.01))
    if voice.boardController!.activeLessonIndex==1 && !sawSecond {
        sawSecond=true;assert(voice.petPose?.hasPrefix("teaching-emphasize")==true)
        voice.boardController!.document.annotationBegan -= 1
        try capture(voice.boardController!.window,"highlight-point")
    }
    assert(voice.subtitle.stringValue==oldCaption)
}
assert(sawSecond && voice.boardSpeechRequest==nil && !voice.activeBoardSpeech)
voice.receive(["type":"board_audio","request_id":"lesson-test","index":2,"data":data.base64EncodedString()]);assert(voice.boardAudioQueue.isEmpty)
voice.boardController!.beginSegment(LessonAudio(data:data,caption:"ここが大切なポイントです。",focus:"chart",gesture:"point",index:2))
try capture(voice.boardController!.captions.panel,"japanese-caption")
let longCaption=String(repeating:"这段较长的字幕也必须完整显示，不受板书滚动或屏幕边缘影响。",count:6)
voice.boardController!.beginSegment(LessonAudio(data:data,caption:longCaption,focus:"point-0",gesture:"explain",index:3))
voice.boardController!.window.setFrameOrigin(NSScreen.main!.visibleFrame.origin)
voice.boardController!.captions.layout(below:voice.boardController!.window)
assert(NSScreen.main!.visibleFrame.contains(captionWindow.frame))
assert(captionWindow.frame.maxY < voice.boardController!.window.frame.minY)
assert(voice.boardController!.lessonCaption.frame.height>40)
try capture(captionWindow,"long-caption")
voice.stopBoardSpeech();assert(voice.boardController!.document.focusRange==nil)
voice.boardController!.window.close()
assert(!captionWindow.isVisible,"Closing a board must also hide its captions")
let briefID="brief-"+UUID().uuidString
voice.preferences.taskDelivery="manual"
voice.receive(["type":"task_update","task":["id":briefID,"status":"completed","presentation":"brief","title":"运行测试","result":"测试通过。","updated":1.0]])
assert(!voice.autoAwaitingBoards.contains(briefID) && voice.pendingBriefTasks.isEmpty)
voice.showBriefResult(briefID);assert(voice.subtitle.stringValue.contains("测试通过。") && !voice.boardController!.window.isVisible)
voice.shutdown()
print("PASS: @ and / keyboard autocomplete; project scope; Unicode and email boundaries; native sequential audio; synchronized captions/focus/gesture; late audio rejection; Japanese subtitles")
