import Cocoa
import AVFoundation
let app = NSApplication.shared; app.setActivationPolicy(.accessory)
let root = URL(fileURLWithPath: CommandLine.arguments[1])
let voice = VoiceController(root: root, startWorker: false)
voice.streamPlayer.engine.mainMixerNode.outputVolume = 0
assert(voice.petPose == nil)
voice.currentID = "a"; voice.generating = true
assert(voice.petPose == "chat-thinking")
voice.receive(["type":"user","id":"a","text":"你真可爱"])
voice.receive(["type":"delta","id":"a","text":"谢谢你。我们继续吧。"])
let caption = voice.subtitle.stringValue
func queue(_ id: String, _ expression: String) {
    voice.receive(["type":"audio_start","id":"a","stream":id,"sample_rate":32000,"subtitle":id,"expression":expression])
    voice.receive(["type":"audio_chunk","id":"a","stream":id,"data":Data(repeating:0,count:16000).base64EncodedString()])
    voice.receive(["type":"audio_end","id":"a","stream":id])
}
queue("one", "shy")
assert(voice.petPose == "chat-shy")
queue("two", "nod")
assert(voice.petPose == "chat-shy", "A queued second sentence must not change the active expression")
voice.receive(["type":"done","id":"a"])
assert(voice.petPose == "chat-shy", "Model completion must not interrupt playing expression")
let until = Date().addingTimeInterval(3)
while Date() < until && voice.petPose != "chat-nod" { RunLoop.main.run(until: Date().addingTimeInterval(0.01)) }
assert(voice.petPose == "chat-nod")
assert(voice.subtitle.stringValue == caption)
voice.stopAll(); assert(voice.petPose == nil)
voice.receive(["type":"audio_start","id":"a","stream":"late","sample_rate":32000,"expression":"shy"])
assert(voice.petPose == nil && !voice.streamPlayer.isBusy)
voice.currentID = "b"; voice.generating = true
voice.receive(["type":"error","id":"b","message":"连接失败"])
assert(voice.petPose == "failed")
voice.stopAll()
let temp = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
defer { try? FileManager.default.removeItem(at: temp) }
let url = temp.appendingPathComponent("memory.json")
try CompanionNotes(enabled: true, notes: "喜欢简短回复").save(url)
assert((try? CompanionNotes.read(url).notes) == "喜欢简短回复")
let attrs = try FileManager.default.attributesOfItem(atPath: url.path)
assert((attrs[.posixPermissions] as? NSNumber)?.intValue == 0o600)
let editor = CompanionMemoryController(url: url); editor.show()
assert(editor.editor.string == "喜欢简短回复")
let qa = root.appendingPathComponent("qa/companion-behavior")
try FileManager.default.createDirectory(at: qa, withIntermediateDirectories: true)
for mode in [NSAppearance.Name.aqua, .darkAqua] {
    editor.window.appearance = NSAppearance(named: mode)
    let view = editor.window.contentView!; view.layoutSubtreeIfNeeded()
    let bitmap = view.bitmapImageRepForCachingDisplay(in: view.bounds)!
    editor.window.appearance!.performAsCurrentDrawingAppearance { view.displayIfNeeded(); view.cacheDisplay(in: view.bounds, to: bitmap) }
    try bitmap.representation(using: .png, properties: [:])!.write(to: qa.appendingPathComponent("memory-\(mode.rawValue).png"))
}
editor.clearDraft(); assert((try? CompanionNotes.read(url).notes) == "喜欢简短回复")
editor.saveNotes(); assert((try? CompanionNotes.read(url).notes.isEmpty) == true)
editor.window.orderOut(nil); voice.shutdown()
print("PASS: thinking, audio-timed expressions, queued emotion isolation, model completion, stop/stale events, caption stability, editable private memory")
