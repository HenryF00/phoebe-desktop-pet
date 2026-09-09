import Cocoa
let app=NSApplication.shared;app.setActivationPolicy(.accessory)
let root=URL(fileURLWithPath:CommandLine.arguments[1])
let qa=root.appendingPathComponent("qa/lesson")
let controller=TeachingBoardController()
var value=try JSONSerialization.jsonObject(with:Data(contentsOf:root.appendingPathComponent("qa/blackboard/model.json"))) as! [String:Any]
let market=try JSONSerialization.jsonObject(with:Data(contentsOf:qa.appendingPathComponent("market-nvda.json"))) as! [String:Any]
value["metrics"]=[];value["points"]=[["heading":"把价格与经营分开看","explanation":"这里展示的是历史日线，不能把区间涨跌当成未来承诺。"]]
value["title"]="NVDA · 股价与基本面";value["takeaway"]="先看价格走势，再对照公司的经营表现。";value["market_chart"]=market
controller.show(value)
func capture(_ name:String) throws {
    let view=controller.window.contentView!;view.layoutSubtreeIfNeeded();RunLoop.main.run(until:Date().addingTimeInterval(0.1))
    let image=view.bitmapImageRepForCachingDisplay(in:view.bounds)!
    controller.window.appearance!.performAsCurrentDrawingAppearance { view.cacheDisplay(in:view.bounds,to:image) }
    try image.representation(using:.png,properties:[:])!.write(to:qa.appendingPathComponent(name+".png"))
}
assert(controller.status.isHidden && controller.spinner.isHidden)
assert(controller.settingsButton.superview != nil && controller.replay.superview==nil)
try capture("board-refresh-candles")
controller.toggleChartStyle();assert(!controller.candleChart)
try capture("board-refresh-area")
controller.beginSegment(LessonAudio(data:Data(),caption:"这里是今天要看的重点。",focus:"point-0",gesture:"point",index:0))
controller.document.annotationBegan -= 1
try capture("board-refresh-chalk")
controller.setBusy(true);assert(!controller.spinner.isHidden && controller.status.isHidden)
try capture("board-refresh-loading")
controller.beginSegment(LessonAudio(data:Data(),caption:"講義を始めましょう。",focus:"chart",gesture:"point",index:1))
assert(controller.spinner.isHidden && controller.captions.panel.isVisible)
controller.showError("行情暂不可用");assert(!controller.status.isHidden && controller.spinner.isHidden)
controller.setBusy(false)
controller.window.setContentSize(NSSize(width:720,height:560));try capture("board-refresh-small")
controller.toggleOriginal();assert(controller.document.string.contains("完整结果"))
let manifest=try JSONDecoder().decode(AnimationManifest.self,from:Data(contentsOf:root.appendingPathComponent("assets/pet/animations.json")))
for key in manifest.clips.keys.filter({$0.hasPrefix("teaching-")}) {
    let clip=manifest.clips[key]!
    assert(clip.frames.allSatisfy({$0.contains("teaching-pointer")}))
    for path in clip.frames {
        let rep=NSBitmapImageRep(data:try Data(contentsOf:root.appendingPathComponent("assets/pet/"+path)))!
        assert(rep.hasAlpha && rep.pixelsWide==1440 && rep.pixelsHigh==1400)
        assert(rep.colorAt(x:0,y:0)!.alphaComponent==0)
    }
}
controller.window.close()
print("PASS: slate board, one settings button, spinner-only loading, charts/styles/ranges, captions, pointer frames and alpha")
