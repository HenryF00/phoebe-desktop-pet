import Cocoa

let app=NSApplication.shared;app.setActivationPolicy(.accessory)
let root=URL(fileURLWithPath:CommandLine.arguments[1])
let voice=VoiceController(root:root,startWorker:false)
voice.preferences.boardSpeech=false
voice.preferences.subtitleLanguage="zh";voice.preferences.voiceLanguage="ja"
voice.preferences.taskDelivery="auto"
func board(_ id:String)->[String:Any] {
    ["task_id":id,"title":"课堂追问","takeaway":"接着解释这个问题。","points":[],"original":"原文",
     "subtitle_language":"zh","voice_language":"ja","closing_caption":"这一部分就讲到这里啦。还有哪里想听我再讲讲？"]
}
voice.show();assert(voice.composerShown)
voice.boards["A"]=board("A");voice.openBoard("A")
let window=voice.boardController!.window,footer=voice.boardController!.captions
assert(voice.lectureMode && !voice.window.isVisible && !voice.bubble.isVisible)
assert(footer.panel.isVisible && footer.input.isEnabled && footer.input.placeholderString==nil)
voice.show();assert(!voice.window.isVisible,"Pet click focuses classroom input while teaching")
voice.currentID="followup";voice.generating=true;voice.lectureCurrentTurn=true
footer.setInputEnabled(false)
voice.receive(["type":"user","id":"followup","text":"那为什么呢？"])
assert(footer.userText.stringValue=="那为什么呢？","Recognized or typed question remains visible")
voice.receive(["type":"lesson","id":"followup","job":["id":"chat-B","parent_id":"A","result":"第二轮解释"]])
assert(voice.requestedBoard=="chat-B" && voice.boardController!.board["task_id"] as? String=="A")
voice.receive(["type":"board_error","task_id":"A","request_id":"old-audio","message":"过期错误"])
voice.receive(["type":"board_error","task_id":"A","message":"过期整理错误"])
assert(voice.requestedBoard=="chat-B" && !footer.input.isEnabled,"Stale errors cannot erase or unlock a pending followup")
voice.receive(["type":"board_ready","board":board("chat-B")])
assert(voice.boardController!.window === window && voice.boardController!.board["task_id"] as? String=="chat-B")
assert(footer.input.isEnabled && footer.userText.stringValue=="那为什么呢？")
voice.boardController!.endLesson(completed:true)
let closing=footer.label.stringValue
voice.showSubtitle("普通聊天字幕")
assert(footer.label.stringValue==closing && !voice.bubble.isVisible)
voice.currentID="cancel";voice.generating=true;voice.requestedBoard="chat-C";footer.setInputEnabled(false)
voice.stopAll()
assert(footer.input.isEnabled && voice.currentID==nil && voice.requestedBoard==nil,"Stopping or opening settings releases classroom input")
voice.requestedBoard="chat-D";voice.autoAwaitingBoards.insert("chat-D")
window.close()
assert(!voice.lectureMode && !footer.panel.isVisible && voice.window.isVisible)
voice.receive(["type":"board_ready","board":board("chat-D")]);voice.presentationTick()
assert(!window.isVisible && !voice.lectureMode,"A late result cannot reopen a dismissed classroom")
voice.shutdown()
print("PASS: classroom entry, input routing, recognized question retention, same-window followup, closing subtitle isolation, cancel recovery and late-event rejection")
