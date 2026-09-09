import Cocoa

final class BlackboardBackground: NSView {
    override func draw(_ dirtyRect:NSRect) {
        let frame=NSBezierPath(roundedRect:bounds.insetBy(dx:1,dy:1),xRadius:8,yRadius:8)
        NSGradient(colors:[NSColor(calibratedRed:0.33,green:0.24,blue:0.16,alpha:1),NSColor(calibratedRed:0.19,green:0.13,blue:0.085,alpha:1)])!.draw(in:frame,angle:90)
        let panel=NSBezierPath(roundedRect:bounds.insetBy(dx:10,dy:10),xRadius:3,yRadius:3)
        NSGradient(colors:[NSColor(calibratedRed:0.11,green:0.19,blue:0.16,alpha:1),NSColor(calibratedRed:0.065,green:0.12,blue:0.105,alpha:1)])!.draw(in:panel,angle:115)
        NSGraphicsContext.saveGraphicsState();panel.addClip()
        // Stationary fine slate grain, never redrawn with random seeds.
        for i in 0..<6500 {
            let x=CGFloat((i &* 15485863)%10007)/10007*bounds.width
            let y=CGFloat((i &* 32452843)%10009)/10009*bounds.height
            NSColor.white.withAlphaComponent(i%3==0 ? 0.024:0.012).setFill()
            NSRect(x:x,y:y,width:1.2,height:0.6).fill()
        }
        NSGraphicsContext.restoreGraphicsState()
        NSColor.black.withAlphaComponent(0.35).setStroke();panel.lineWidth=2;panel.stroke()
        NSColor.white.withAlphaComponent(0.12).setStroke();frame.lineWidth=1;frame.stroke()
    }
}

/// A local, selectable teaching document; task text is never interpreted as HTML or code.
final class TeachingBoardController: NSObject, NSWindowDelegate {
    let window = NSWindow(contentRect:NSRect(x:0,y:0,width:880,height:760),styleMask:[.titled,.closable,.resizable],backing:.buffered,defer:false)
    let document = LessonTextView()
    let captions = LessonCaptions()
    var lessonCaption:NSTextField { captions.label }
    var focusRanges: [String:NSRange] = [:]
    var activeFocus: String?
    var activeLessonIndex = -1
    let status = NSTextField(labelWithString:"")
    let spinner=NSProgressIndicator()
    let settingsButton=NSButton()
    var onSettings:(()->Void)?
    var onFollowUp:((String)->Void)?
    var onRecordBegin:(()->Void)?
    var onRecordEnd:(()->Void)?
    var onLayout:(()->Void)?
    var chartDays=60
    var candleChart=true
    var busy=false
    func setBusy(_ value:Bool) {
        busy=value;spinner.isHidden = !value
        if value { spinner.startAnimation(nil) } else { spinner.stopAnimation(nil) }
        status.stringValue="";status.isHidden=true
    }
    func showError(_ message:String) {
        setBusy(false);status.stringValue=message;status.isHidden=message.isEmpty
    }
    let replay = NSButton(title:"听重点讲解",target:nil,action:nil)
    let original = NSButton(title:"完整结果",target:nil,action:nil)
    let retry = NSButton(title:"重新整理",target:nil,action:nil)
    var board: [String:Any] = [:]
    var showingOriginal = false
    var onSpeak: ((String)->Void)?
    var onRetry: ((String)->Void)?
    var onStop: (()->Void)?
    var onClose: (()->Void)?
    let ink = NSColor(calibratedRed:0.89,green:0.95,blue:0.94,alpha:1)
    let accent = NSColor(calibratedRed:0.49,green:0.82,blue:0.94,alpha:1)
    override init() {
        super.init()
        captions.onSubmit = { [weak self] text in self?.onFollowUp?(text) }
        captions.onRecordBegin = { [weak self] in self?.onRecordBegin?() }
        captions.onRecordEnd = { [weak self] in self?.onRecordEnd?() }
        captions.onLayout = { [weak self] in self?.onLayout?() }
        window.title="洛琪希的小黑板";window.titlebarAppearsTransparent=true;window.isReleasedWhenClosed=false;window.delegate=self
        window.minSize=NSSize(width:720,height:560);window.appearance=NSAppearance(named:.darkAqua);window.center()
        window.backgroundColor=NSColor(calibratedRed:0.065,green:0.12,blue:0.135,alpha:1)
        window.contentView=BlackboardBackground(frame:window.contentView!.bounds)
        document.drawsBackground=false;document.isEditable=false;document.isSelectable=true;document.isRichText=true
        document.textContainerInset=NSSize(width:32,height:24);document.autoresizingMask=[.width]
        document.isVerticallyResizable=true;document.textContainer?.widthTracksTextView=true
        document.setAccessibilityLabel("小黑板讲解，包含要点、图表和来源")
        document.linkTextAttributes=[.foregroundColor:accent,.underlineStyle:NSUnderlineStyle.single.rawValue]
        let scroll=NSScrollView();scroll.drawsBackground=false;scroll.hasVerticalScroller=true;scroll.documentView=document
        for (button,selector) in [(replay,#selector(speak)),(original,#selector(toggleOriginal)),(retry,#selector(rebuild))] {
            button.target=self;button.action=selector;button.bezelStyle = .rounded
        }
        settingsButton.image=NSImage(systemSymbolName:"gearshape",accessibilityDescription:"小黑板设置")
        settingsButton.imagePosition = .imageOnly;settingsButton.isBordered=false
        settingsButton.contentTintColor=ink;settingsButton.target=self;settingsButton.action=#selector(openSettingsMenu)
        settingsButton.toolTip="小黑板设置";settingsButton.setAccessibilityLabel("小黑板设置")
        settingsButton.widthAnchor.constraint(equalToConstant:36).isActive=true
        settingsButton.heightAnchor.constraint(equalToConstant:36).isActive=true
        spinner.style = .spinning;spinner.controlSize = .small;spinner.isDisplayedWhenStopped=false;spinner.isHidden=true
        spinner.setAccessibilityLabel("正在准备讲解")
        let heading=NSTextField(labelWithString:"洛琪希的小黑板");heading.font=BoardTypography.font(18);heading.textColor=ink.withAlphaComponent(0.75)
        let bar=NSStackView(views:[heading,NSView(),spinner,settingsButton]);bar.spacing=8
        status.textColor=NSColor(calibratedRed:1,green:0.7,blue:0.65,alpha:1);status.font = .systemFont(ofSize:12);status.isHidden=true
        let stack=NSStackView(views:[bar,scroll,status]);stack.orientation = .vertical;stack.alignment = .leading;stack.spacing=4
        stack.translatesAutoresizingMaskIntoConstraints=false;window.contentView!.addSubview(stack)
        NSLayoutConstraint.activate([stack.leadingAnchor.constraint(equalTo:window.contentView!.leadingAnchor,constant:24),
            stack.trailingAnchor.constraint(equalTo:window.contentView!.trailingAnchor,constant:-24),
            stack.topAnchor.constraint(equalTo:window.contentView!.topAnchor,constant:18),
            stack.bottomAnchor.constraint(equalTo:window.contentView!.bottomAnchor,constant:-24)])
        for view in stack.arrangedSubviews { view.widthAnchor.constraint(equalTo:stack.widthAnchor).isActive=true }
    }
    func show(_ value:[String:Any], activate:Bool=true) {
        if value["task_id"] as? String != board["task_id"] as? String || value["signature"] as? String != board["signature"] as? String {
            activeFocus=nil;activeLessonIndex = -1;document.focus(nil)
            lessonCaption.stringValue=""
        }
        board=value;showingOriginal=false;setBusy(value["preparing"] as? Bool == true);render()
        if activate { window.makeKeyAndOrderFront(nil);NSApp.activate(ignoringOtherApps:true) }
        else { window.orderFrontRegardless() }
        captions.show(lessonCaption.stringValue,below:window)
    }
    func render() {
        let content=NSMutableAttributedString(string:"")
        focusRanges.removeAll()
        func line(_ text:String,_ size:CGFloat=16,_ weight:NSFont.Weight = .regular,_ color:NSColor?=nil) {
            let p=NSMutableParagraphStyle();p.lineSpacing=5;p.paragraphSpacing=12
            content.append(NSAttributedString(string:text+"\n",attributes:[.font:BoardTypography.font(size+2,weight:weight),.foregroundColor:color ?? ink,.paragraphStyle:p]))
        }
        original.title=showingOriginal ? "返回小黑板":"完整结果"
        replay.isEnabled = !(board["narration"] as? [[String:String]] ?? []).isEmpty
        retry.isEnabled = board["task_id"] != nil
        if showingOriginal {
            line("完整结果与原始来源",25,.semibold)
            line(board["original"] as? String ?? "",14)
            if let source=board["root_source"] as? String,!source.isEmpty,source != board["original"] as? String {
                line("本课原始资料",20,.semibold)
                line(source,14)
            }
            if let market=board["market_chart"] as? [String:Any] {
                line("独立历史行情快照 · "+(market["symbol"] as? String ?? ""),20,.semibold)
                line("Nasdaq · USD · 未自行复权 · 截至 "+(market["as_of"] as? String ?? ""),13)
                line("日期 / 开盘 / 最高 / 最低 / 收盘 / 成交量",13)
                for row in market["rows"] as? [[String:Any]] ?? [] {
                    let values=["open","high","low","close","volume"].map { key in String(format:"%g",(row[key] as? NSNumber)?.doubleValue ?? 0) }
                    line((row["date"] as? String ?? "")+" / "+values.joined(separator:" / "),12)
                }
            }
        } else {
            line(board["title"] as? String ?? "正在整理讲解…",28,.semibold)
            let takeawayStart=content.length
            line(board["takeaway"] as? String ?? "",19,.medium,accent)
            focusRanges["takeaway"]=NSRange(location:takeawayStart,length:content.length-takeawayStart-1)
            if let market=board["market_chart"] as? [String:Any] {
                let attachment=NSTextAttachment();attachment.image=MarketChartDrawing.image(market,days:chartDays,candles:candleChart,width:min(700,window.frame.width-132))
                focusRanges["chart"]=NSRange(location:content.length,length:1)
                content.append(NSAttributedString(attachment:attachment));line("",8)
            }
            let metrics=board["metrics"] as? [[String:String]] ?? []
            if !metrics.isEmpty {
                line("关键数据",12,.semibold,.secondaryLabelColor)
                for (index,metric) in metrics.enumerated() {
                    let start=content.length
                    line("\(metric["label"] ?? "")：\(metric["value"] ?? "")  ·  \(metric["context"] ?? "")",15,.medium)
                    focusRanges["metric-\(index)"]=NSRange(location:start,length:content.length-start-1)
                }
            }
            if board["market_chart"] == nil,let chart=board["chart"] as? [String:Any],let points=chart["points"] as? [[String:Any]],points.count>=2 {
                line(chart["title"] as? String ?? "数据对比",17,.semibold)
                let attachment=NSTextAttachment();attachment.image=chartImage(points,unit:chart["unit"] as? String ?? "")
                focusRanges["chart"]=NSRange(location:content.length,length:1)
                content.append(NSAttributedString(attachment:attachment));line("",4)
            }
            for (index,point) in (board["points"] as? [[String:String]] ?? []).enumerated() {
                let start=content.length
                line(String(format:"%02d",index+1)+"   "+(point["heading"] ?? ""),19,.semibold,accent)
                focusRanges["point-\(index)"]=NSRange(location:start,length:content.length-start-1)
                line(point["explanation"] ?? "",16)
            }

        }
        let links=board["sources"] as? [String] ?? []
        if showingOriginal && !links.isEmpty {
            line("来源 · 点击核对原文",12,.semibold,.secondaryLabelColor)
            for link in links {
                guard let url=URL(string:link),["https","http"].contains(url.scheme ?? "") else { continue }
                content.append(NSAttributedString(string:link+"\n\n",attributes:[.link:url,.font:NSFont.systemFont(ofSize:12),.foregroundColor:accent]))
            }
        }
        document.textStorage?.setAttributedString(content)
        if let focus=activeFocus,!showingOriginal { document.focus(focusRanges[focus]) }
        else { document.focus(nil);document.scrollToBeginningOfDocument(nil) }
        if board["fallback"] as? Bool == true { showError(board["prepare_error"] as? String ?? "讲解暂不可用，请从设置查看完整结果。") }
        else if let error=board["market_chart_error"] as? String { showError(error) }
    }
    func chartImage(_ points:[[String:Any]],unit:String)->NSImage {
        let width:CGFloat=590, row:CGFloat=52, height=CGFloat(points.count)*row+30
        return NSImage(size:NSSize(width:width,height:height),flipped:true) { [self] rect in
            let values=points.map { ($0["value"] as? Double) ?? 0 }
            let low=min(0,values.min() ?? 0), high=max(0,values.max() ?? 1)
            let range=max(1e-9,high-low), left:CGFloat=140, area:CGFloat=320
            let zero=left+CGFloat((0-low)/range)*area
            let axis=NSBezierPath();axis.move(to:NSPoint(x:zero,y:8));axis.line(to:NSPoint(x:zero,y:height-25));axis.lineWidth=1
            NSColor.white.withAlphaComponent(0.25).setStroke();axis.stroke()
            for (i,point) in points.enumerated() {
                let y=CGFloat(i)*row+8, value=values[i], end=left+CGFloat((value-low)/range)*area
                (point["label"] as? String ?? "").draw(in:NSRect(x:0,y:y+4,width:130,height:40),withAttributes:[.font:BoardTypography.font(15),.foregroundColor:ink])
                accent.withAlphaComponent(i==points.count-1 ? 1:0.58).setFill()
                NSBezierPath(roundedRect:NSRect(x:min(zero,end),y:y,width:max(1,abs(end-zero)),height:25),xRadius:3,yRadius:3).fill()
                let formatted=(point["display_value"] as? String ?? String(format:"%g",value))+" "+unit
                formatted.draw(in:NSRect(x:470,y:y+4,width:120,height:40),withAttributes:[.font:NSFont.monospacedDigitSystemFont(ofSize:13,weight:.medium),.foregroundColor:ink])
            }
            ("0 · "+unit).draw(at:NSPoint(x:zero,y:height-20),withAttributes:[.font:NSFont.systemFont(ofSize:11),.foregroundColor:ink.withAlphaComponent(0.7)])
            return true
        }
    }
    func beginSegment(_ segment:LessonAudio) {
        activeFocus=segment.focus;activeLessonIndex=segment.index
        captions.show(segment.caption,below:window)
        if !showingOriginal { document.focus(segment.focus == "closing" ? nil:(focusRanges[segment.focus] ?? focusRanges["takeaway"])) }
        setBusy(false)
    }
    func endLesson(completed:Bool) {
        if !completed { activeFocus=nil;document.focus(nil) }
        else if let closing=board["closing_caption"] as? String,!closing.isEmpty { captions.show(closing,below:window) }
        setBusy(false)
    }
    @objc func openSettingsMenu() {
        let menu=NSMenu()
        @discardableResult func item(_ title:String,_ action:Selector)->NSMenuItem {
            let item=NSMenuItem(title:title,action:action,keyEquivalent:"");item.target=self;menu.addItem(item);return item
        }
        item("听重点讲解",#selector(speak)).isEnabled=replay.isEnabled
        item("停止讲解",#selector(stopSpeaking))
        menu.addItem(.separator())
        item(showingOriginal ? "返回小黑板":"完整结果与来源",#selector(toggleOriginal))
        item("重新整理",#selector(rebuild)).isEnabled=retry.isEnabled
        if board["market_chart"] != nil {
            menu.addItem(.separator())
            item(candleChart ? "切换为走势面积图":"切换为 K 线图",#selector(toggleChartStyle))
            for days in [20,60,120] {
                let row=item("最近 \(days) 个交易日",#selector(changeRange(_:)));row.tag=days;row.state=days==chartDays ? .on:.off
            }
        }
        menu.addItem(.separator());item("字幕、声音与其他设置…",#selector(showAllSettings))
        menu.popUp(positioning:nil,at:NSPoint(x:settingsButton.bounds.maxX,y:settingsButton.bounds.minY),in:settingsButton)
    }
    @objc func toggleChartStyle() { candleChart.toggle();render() }
    @objc func changeRange(_ sender:NSMenuItem) { chartDays=sender.tag;render() }
    @objc func showAllSettings() { onSettings?() }
    @objc func speak() { if let id=board["task_id"] as? String { onSpeak?(id) } }
    @objc func rebuild() { if let id=board["task_id"] as? String { onRetry?(id) } }
    @objc func stopSpeaking() { onStop?() }
    @objc func toggleOriginal() { showingOriginal.toggle();render() }
    func windowDidMove(_ notification:Notification) { if !captions.positioning { captions.layout(below:window) } }
    func windowDidResize(_ notification:Notification) { if board["market_chart"] != nil { render() };if !captions.positioning { captions.layout(below:window) } }
    func windowWillMiniaturize(_ notification:Notification) { captions.hide() }
    func windowDidDeminiaturize(_ notification:Notification) { captions.show(lessonCaption.stringValue,below:window) }
    func windowWillClose(_ notification:Notification) { captions.hide();onClose?() }
}
