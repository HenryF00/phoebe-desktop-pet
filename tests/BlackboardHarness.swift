import Cocoa
import AVFoundation
let app=NSApplication.shared;app.setActivationPolicy(.accessory)
let root=URL(fileURLWithPath:CommandLine.arguments[1])
let qa=root.appendingPathComponent("qa/blackboard")
try FileManager.default.createDirectory(at:qa,withIntermediateDirectories:true)
let board=try JSONSerialization.jsonObject(with:Data(contentsOf:qa.appendingPathComponent("model.json"))) as! [String:Any]
let voice=VoiceController(root:root,startWorker:false)
voice.preferences.taskDelivery="manual";voice.preferences.boardSpeech=false
voice.preferences.voiceLanguage="ja";voice.preferences.subtitleLanguage="zh"
voice.currentAnswer="当前聊天字幕保持不变。";voice.renderReplyText()
let caption=voice.subtitle.stringValue
let id=board["task_id"] as! String
let oldSeen=UserDefaults.standard.object(forKey:"blackboardReadRevisions")
UserDefaults.standard.removeObject(forKey:"blackboardReadRevisions")
defer { UserDefaults.standard.set(oldSeen,forKey:"blackboardReadRevisions") }
var job:[String:Any]=["id":id,"title":board["title"]!,"status":"running","updated":1.0,"result":""]
voice.receive(["type":"task_update","task":job]);assert(voice.taskBadgeState=="running")
job["status"]="completed";job["updated"]=2.0;job["result"]=board["original"]
voice.receive(["type":"task_update","task":job]);voice.receive(["type":"board_ready","board":board])
assert(voice.taskBadgeState=="unread" && voice.boardController==nil && voice.subtitle.stringValue==caption)
assert(voice.boardSpeechRequest==nil && voice.automaticBoards.isEmpty,"Manual completion must remain silent")
voice.showTaskIndicator()
assert(voice.boardController!.window.isVisible && voice.taskBadgeState=="none")
assert(voice.subtitle.stringValue==caption)
voice.boardController!.toggleOriginal();assert(voice.boardController!.document.string.contains("以下全部"))
voice.boardController!.toggleOriginal()
func capture(_ window:NSWindow,_ name:String) throws {
    let view=window.contentView!;view.layoutSubtreeIfNeeded()
    RunLoop.main.run(until:Date().addingTimeInterval(0.12))
    let bitmap=view.bitmapImageRepForCachingDisplay(in:view.bounds)!
    (window.appearance ?? NSAppearance.currentDrawing()).performAsCurrentDrawingAppearance {
        view.displayIfNeeded();view.cacheDisplay(in:view.bounds,to:bitmap)
    }
    try bitmap.representation(using:.png,properties:[:])!.write(to:qa.appendingPathComponent(name+".png"))
}
try capture(voice.boardController!.window,"blackboard")
voice.boardController!.window.close()
voice.preferences.taskDelivery="auto";voice.generating=true
job["updated"]=3.0
voice.notifiedTasks.remove(id);voice.receive(["type":"task_update","task":job])
voice.receive(["type":"board_ready","board":board]);voice.presentationTick()
assert(!voice.boardController!.window.isVisible,"Auto board must wait through chat generation")
voice.generating=false;voice.presentationTick()
assert(voice.boardController!.window.isVisible && voice.taskBadgeState=="none")
// A stale audio result after cancellation cannot enter the playback queue.
voice.boardSpeechRequest="new";voice.receive(["type":"board_audio","request_id":"old","data":Data([0,0]).base64EncodedString()])
assert(voice.boardAudio==nil)
voice.stopBoardSpeech()
// Real AVAudioPlayer queue: synthesize completion cannot start during an unfinished chat.
let wav=try Data(contentsOf:root.appendingPathComponent("runtime/blackboard-qa-short.wav"))
voice.generating=true;voice.boardSpeechRequest="audio-test"
voice.receive(["type":"board_audio","request_id":"audio-test","subtitle":"讲解重点","data":wav.base64EncodedString()])
assert(voice.player==nil && voice.boardAudio != nil)
voice.receive(["type":"board_audio_done","request_id":"audio-test"])
voice.generating=false;voice.playNext();voice.player?.volume=0
assert(voice.player != nil && voice.activeBoardSpeech && voice.subtitle.stringValue==caption)
let deadline=Date().addingTimeInterval(4)
while voice.player != nil && Date()<deadline { RunLoop.main.run(until:Date().addingTimeInterval(0.02)) }
assert(voice.player==nil && voice.boardSpeechRequest==nil && voice.subtitle.stringValue==caption)
voice.boardController!.window.close()
voice.showSettings()
assert(voice.settingsHome!.tabs.numberOfTabViewItems==4)
assert(voice.settingsController!.window.isVisible==false && voice.memoryController!.window.isVisible==false)
for index in 0..<4 {
    voice.settingsHome!.tabs.selectTabViewItem(at:index)
    try capture(voice.settingsHome!.window,"settings-\(index)")
}
// Badge hit testing includes transparent pixels; click does not trigger chat or dragging.
let pet=PetView(frame:NSRect(x:0,y:0,width:600,height:584))
pet.image=NSImage(contentsOf:root.appendingPathComponent("assets/pet/frames/master.png"))
let panel=PetPanel(contentRect:pet.frame,styleMask:[.borderless],backing:.buffered,defer:false);panel.contentView=pet
var clicked=false;pet.taskAction={clicked=true}
for state in ["running","waiting","unread"] {
    pet.taskBadge=state;pet.taskUnread=true
    let point=NSPoint(x:pet.badgeRect.midX,y:pet.badgeRect.midY)
    assert(pet.isOpaqueAt(point))
    let event=NSEvent.mouseEvent(with:.leftMouseDown,location:point,modifierFlags:[],timestamp:0,windowNumber:panel.windowNumber,context:nil,eventNumber:1,clickCount:1,pressure:1)!
    pet.mouseDown(with:event);pet.mouseUp(with:event);assert(clicked && pet.dragStart==nil)
    try capture(panel,"badge-\(state)")
}
voice.shutdown();voice.settingsHome!.window.orderOut(nil)
print("PASS: running/unread/read states; silent manual results; automatic waits for chat; original result; stale audio rejected; four embedded settings pages; badge hit testing")
