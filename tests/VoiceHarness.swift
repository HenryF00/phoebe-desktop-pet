import Cocoa
import AVFoundation

let app = NSApplication.shared
app.setActivationPolicy(.accessory)
let root = URL(fileURLWithPath: CommandLine.arguments[1])
let voice = VoiceController(root: root, startWorker: false)
voice.currentID = "one"; voice.generating = true
voice.receive(["type":"user", "id":"one", "text":"你好"])
voice.receive(["type":"delta", "id":"one", "text":"こんにちは。"])
// A concurrent notice must not divert subsequent reply tokens into its paragraph.
voice.append("任务提醒", "確認してください。")
voice.receive(["type":"delta", "id":"one", "text":"元気ですか？"])
assert(voice.transcript.string.contains("こんにちは。元気ですか？\n\n任务提醒"))
voice.receive(["type":"done", "id":"one", "text":"こんにちは。元気ですか？"])
assert(voice.history.count == 2)
voice.stopAll()
let before = voice.transcript.string
voice.receive(["type":"delta", "id":"one", "text":"late"])
voice.receive(["type":"audio", "id":"one", "data":Data([1,2,3]).base64EncodedString()])
assert(voice.transcript.string == before && voice.clips.isEmpty && voice.player == nil)
voice.clearChat(); assert(voice.history.isEmpty && voice.currentID == nil)
voice.receive(["type":"hooks", "connected":false])
voice.phase.stringValue = "请在系统设置 → 隐私与安全性 → 麦克风中允许洛琪希；也可打字聊天。"
let area = NSRect(x:0,y:0,width:1440,height:900)
for x in [CGFloat(0), 1250] {
    let anchor = NSRect(x:x,y:200,width:180,height:500)
    let (frame, left) = companionFrame(anchor:anchor, size:NSSize(width:360,height:260), screen:area)
    assert(area.contains(frame)); assert(left == (x > 0))
}
voice.anchorProvider = { (NSRect(x:1200,y:40,width:180,height:550), area) }
voice.showSubtitle("今天辛苦了。我们可以一起聊聊魔法，也可以安静地陪你休息一会儿。", reveal:false)
assert(voice.subtitle.stringValue.contains("今天辛苦了"))
voice.updateAnchor()
assert(area.contains(voice.window.frame) && area.contains(voice.bubble.frame))
assert(voice.window.frame.height == 54)
assert(voice.textEntry.arrangedSubviews.count == 2)
assert(voice.input.focusRingType == .none && !voice.input.isBordered)
assert(voice.talk.pressed != nil && voice.talk.released != nil)

voice.renderQA(root.appendingPathComponent("qa/companion-ui-long").path)
voice.shutdown()
print("PASS: reply/notice interleaving, context, stop/stale audio, reset, anchor edges, single-row input, caption layout")
