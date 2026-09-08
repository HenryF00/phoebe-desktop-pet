import Cocoa
import AVFoundation
let app = NSApplication.shared
app.setActivationPolicy(.accessory)
let audio = StreamingAudioPlayer()
audio.engine.mainMixerNode.outputVolume = 0
var began = [String](); var idle = 0; var failed = false
audio.onBegan = { began.append($0) }; audio.onIdle = { idle += 1 }; audio.onError = { failed = true }
let pcm = Data(repeating:0,count:6400)
audio.start("one", subtitle:"第一句", sampleRate:32000)
audio.append("one", data:pcm)
assert(audio.node.isPlaying && began == ["第一句"]) // Playback started before end arrived.
audio.append("one", data:pcm); audio.end("one")
audio.start("two", subtitle:"第二句", sampleRate:32000); audio.append("two",data:pcm);audio.end("two")
let deadline=Date().addingTimeInterval(4)
while audio.isBusy && Date() < deadline { RunLoop.main.run(until:Date().addingTimeInterval(0.02)) }
assert(!failed && !audio.isBusy && idle == 1 && began == ["第一句","第二句"])
audio.start("cancel",subtitle:"已取消",sampleRate:32000);audio.append("cancel",data:pcm);audio.stop()
audio.append("cancel",data:pcm);audio.end("cancel")
RunLoop.main.run(until:Date().addingTimeInterval(0.2))
assert(!audio.isBusy && !audio.node.isPlaying)
audio.suspended=true;audio.start("waiting",subtitle:"排队",sampleRate:32000);audio.append("waiting",data:pcm)
assert(!audio.node.isPlaying);audio.suspended=false;assert(audio.node.isPlaying);audio.stop()
print("PASS: real audio-engine starts before stream end; ordered buffers; drain; stop ignores stale callbacks; suspended playback")
