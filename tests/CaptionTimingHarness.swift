import Cocoa
import AVFoundation
let app=NSApplication.shared;app.setActivationPolicy(.accessory)
let root=URL(fileURLWithPath:CommandLine.arguments[1])
let voice=VoiceController(root:root,startWorker:false)
voice.streamPlayer.engine.mainMixerNode.outputVolume=0
voice.currentID="timing";voice.generating=true
voice.receive(["type":"user","id":"timing","text":"请分三句回答"])
voice.receive(["type":"delta","id":"timing","text":"第一句。第二句。"])
let initial=voice.subtitle.stringValue
voice.receive(["type":"audio_start","id":"timing","stream":"first","subtitle":"第一句。","sample_rate":32000])
voice.receive(["type":"audio_chunk","id":"timing","stream":"first","data":Data(repeating:0,count:64000).base64EncodedString()])
voice.receive(["type":"audio_end","id":"timing","stream":"first"])
guard voice.subtitle.stringValue == initial else { print("FAIL: audio start rolled text back:", initial, "->", voice.subtitle.stringValue);exit(1) }
voice.receive(["type":"delta","id":"timing","text":"第三句。"])
let full="第一句。第二句。第三句。"
assert(voice.subtitle.stringValue == full && voice.activeSubtitle == "第一句。")
voice.receive(["type":"audio_start","id":"timing","stream":"second","subtitle":"第二句。第三句。","sample_rate":32000])
voice.receive(["type":"audio_chunk","id":"timing","stream":"second","data":Data(repeating:0,count:19200).base64EncodedString()])
voice.receive(["type":"audio_end","id":"timing","stream":"second"])
voice.receive(["type":"done","id":"timing","text":full])
assert(voice.subtitle.stringValue == full && voice.activeSubtitle == "第一句。")
let deadline=Date().addingTimeInterval(4);var sawSecond=false
while voice.streamPlayer.isBusy && Date()<deadline {
 RunLoop.main.run(until:Date().addingTimeInterval(0.02))
 assert(voice.subtitle.stringValue == full)
 if voice.activeSubtitle == "第二句。第三句。" { sawSecond=true }
}
assert(sawSecond && !voice.streamPlayer.isBusy && voice.subtitle.stringValue == full)
voice.receive(["type":"notice_error","message":"提醒合成失败"])
assert(voice.subtitle.stringValue.hasPrefix(full))
voice.showReplyWhenIdle();assert(voice.subtitle.stringValue.hasPrefix(full))
voice.shutdown();print("PASS: text never rolls back at audio start, segment changes, done or drain; audio order preserved; notice keeps reply")
