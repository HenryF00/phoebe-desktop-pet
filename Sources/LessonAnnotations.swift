import Cocoa

struct LessonAudio {
    var data:Data
    var caption:String
    var focus:String
    var gesture:String
    var index:Int
}

/// Deterministic chalk strokes: the grain never flickers between redraws.
final class LessonTextView: NSTextView {
    var focusRange:NSRange? { didSet { needsDisplay=true } }
    var annotationBegan=ProcessInfo.processInfo.systemUptime
    var annotationTimer:Timer?
    func focus(_ range:NSRange?) {
        focusRange=range;annotationBegan=ProcessInfo.processInfo.systemUptime
        annotationTimer?.invalidate()
        guard range != nil else{return}
        if let range=range { scrollRangeToVisible(range) }
        if !NSWorkspace.shared.accessibilityDisplayShouldReduceMotion {
            annotationTimer=Timer.scheduledTimer(withTimeInterval:1/30,repeats:true) { [weak self] timer in
                guard let self=self else{timer.invalidate();return}
                self.needsDisplay=true
                if ProcessInfo.processInfo.systemUptime-self.annotationBegan>0.65 { timer.invalidate() }
            }
        }
    }
    override func draw(_ dirtyRect:NSRect) {
        NSGraphicsContext.saveGraphicsState();super.draw(dirtyRect);NSGraphicsContext.restoreGraphicsState()
        guard let range=focusRange,range.length>0,NSMaxRange(range)<=textStorage?.length ?? 0,
              let layout=layoutManager,let container=textContainer else{return}
        let glyphs=layout.glyphRange(forCharacterRange:range,actualCharacterRange:nil)
        let rect=layout.boundingRect(forGlyphRange:glyphs,in:container)
            .offsetBy(dx:textContainerOrigin.x,dy:textContainerOrigin.y).insetBy(dx:-14,dy:-9)
        let progress=NSWorkspace.shared.accessibilityDisplayShouldReduceMotion ? 1:min(1,(ProcessInfo.processInfo.systemUptime-annotationBegan)/0.55)
        let count=max(140,Int((rect.width+rect.height)*1.2))
        func point(_ t:Double)->NSPoint {
            let angle=t*2 * Double.pi-Double.pi/2
            let wobble=sin(angle*3+0.4)*1.2+sin(angle*7)*0.55
            let x=cos(angle),y=sin(angle)
            return NSPoint(x:rect.midX+(x<0 ? -1:1)*pow(abs(x),0.75)*(rect.width/2+wobble),
                           y:rect.midY+y*(rect.height/2+wobble))
        }
        func noise(_ seed:Int)->CGFloat {
            CGFloat((UInt64(seed+1) &* 2654435761) % 997)/997
        }
        let chalk=NSColor(calibratedRed:0.92,green:0.34,blue:0.34,alpha:1)
        let stroke=NSBezierPath()
        for i in 0...max(1,Int(Double(count)*progress)) {
            let p=point(Double(i)/Double(count))
            if i==0 { stroke.move(to:p) } else { stroke.line(to:p) }
        }
        stroke.lineWidth=3.2;stroke.lineCapStyle = .round;stroke.lineJoinStyle = .round
        chalk.withAlphaComponent(0.78).setStroke();stroke.stroke()
        // The powder stays INSIDE the continuous stroke; no radial edge spikes.
        for i in 0..<max(1,Int(Double(count)*progress)) {
            let p=point(Double(i)/Double(count))
            let offset=(noise(i+401)-0.5)*1.4
            NSColor(calibratedRed:0.12,green:0.20,blue:0.18,alpha:0.28).setFill()
            NSBezierPath(ovalIn:NSRect(x:p.x+offset,y:p.y+offset,width:0.55,height:0.55)).fill()
        }
    }
    deinit { annotationTimer?.invalidate() }
}

/// Choreography follows playback time, so buffering cannot advance a teaching gesture.
func teachingPose(gesture:String,elapsed:Double,boardOnRight:Bool)->String {
    let suffix=boardOnRight ? "-right":""
    if gesture=="emphasize",elapsed<1.4 { return "teaching-emphasize"+suffix }
    if gesture=="point",elapsed<4 { return "teaching-point"+suffix }
    if elapsed<2.8 { return "teaching-present"+suffix }
    let phase=(elapsed-2.8).truncatingRemainder(dividingBy:12)
    if phase<5.5 { return "teaching-explain"+suffix }
    if phase<9 { return "teaching-point"+suffix }
    return "teaching-present"+suffix
}
