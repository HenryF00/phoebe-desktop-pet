import Cocoa
let app = NSApplication.shared; app.setActivationPolicy(.accessory)
let root = URL(fileURLWithPath: CommandLine.arguments[1])
let pet = PetController()
pet.manifest = try JSONDecoder().decode(AnimationManifest.self, from: Data(contentsOf: root.appendingPathComponent("assets/pet/animations.json")))
for (key, clip) in pet.manifest.clips {
    pet.images[key] = clip.frames.map { NSImage(contentsOf: root.appendingPathComponent("assets/pet/" + $0))! }
}
pet.pet = PetView(frame: NSRect(x:0,y:0,width:600,height:584))
pet.panel = PetPanel(contentRect:pet.pet.frame,styleMask:[.borderless],backing:.buffered,defer:false)
pet.panel.contentView = pet.pet
pet.voice = VoiceController(root: root, startWorker: false)
pet.automatic = false; pet.lastCodexPoll = ProcessInfo.processInfo.systemUptime + 100
pet.codexState = "running"; pet.voice?.generating = true
pet.tick(); assert(pet.state == "chat-thinking")
pet.voice?.streamSpeaking = true; pet.voice?.replyExpression = "shy"
pet.tick(); assert(pet.state == "chat-shy")
pet.voice?.stopAll(); pet.tick(); assert(pet.state == "running")
pet.codexState = nil; pet.voice?.generating = true; pet.tick()
pet.voice?.stopAll(); pet.tick(); assert(pet.state == "idle")
pet.manualUntil = ProcessInfo.processInfo.systemUptime + 5
pet.select("chat-nod"); pet.tick(); assert(pet.state == "chat-nod")
pet.voice?.streamSpeaking = true; pet.voice?.replyExpression = "nod"
pet.tick(); pet.frameElapsed = 8; pet.tick()
assert(pet.frameIndex == 2, "Nod holds the last frame instead of nodding repeatedly")
pet.voice?.stopAll();pet.voice?.activeBoardSpeech=true
pet.voice?.currentLessonAudio=LessonAudio(data:Data(),caption:"上课",focus:"takeaway",gesture:"explain",index:0)
pet.tick();assert(pet.state=="teaching-present","Lecture opens with a raised-hand teaching pose")
assert(teachingPose(gesture:"explain",elapsed:4,boardOnRight:false)=="teaching-explain")
assert(teachingPose(gesture:"explain",elapsed:9,boardOnRight:false)=="teaching-point")
assert(teachingPose(gesture:"point",elapsed:1,boardOnRight:true)=="teaching-point-right")
assert(teachingPose(gesture:"emphasize",elapsed:0,boardOnRight:true)=="teaching-emphasize-right")
pet.renderQA(directory: root.appendingPathComponent("qa/companion-behavior/poses").path)
pet.voice?.shutdown()
print("PASS: chat priority over tasks, resume tasks, release to idle, manual preview, one-shot nod, pose rendering")
