import Cocoa

struct ComposerChoice {
    var title:String
    var detail:String
    var insertion:String
}
struct ComposerQuery {
    var range:NSRange
    var trigger:String
    var filter:String
    static func at(_ text:String,cursor:Int)->ComposerQuery? {
        let value=text as NSString
        guard cursor>=0,cursor<=value.length else { return nil }
        let prefix=value.substring(to:cursor)
        guard let regex=try? NSRegularExpression(pattern:"(?:^|\\s)([@/])([^\\s@/]*)$"),
            let match=regex.firstMatch(in:prefix,range:NSRange(location:0,length:(prefix as NSString).length)) else { return nil }
        let start=match.range(at:1).location
        return ComposerQuery(range:NSRange(location:start,length:cursor-start),trigger:value.substring(with:match.range(at:1)),filter:value.substring(with:match.range(at:2)))
    }
    func choices(_ projects:[[String:String]])->[ComposerChoice] {
        let commands=[ComposerChoice(title:"委托",detail:"交给 Codex 执行，完成后通知",insertion:"/委托 "),
            ComposerChoice(title:"小黑板",detail:"讲清原理、对比或数据，配合授课字幕",insertion:"/小黑板 "),
            ComposerChoice(title:"聊天",detail:"普通对话，本次不使用小黑板",insertion:"/聊天 ")]
        if trigger=="/" { return commands.filter{filter.isEmpty || $0.title.localizedCaseInsensitiveContains(filter)} }
        let result=projects.filter{filter.isEmpty || ($0["name"] ?? "").localizedCaseInsensitiveContains(filter)}.map {
            let name=$0["name"] ?? "项目"
            let token=name.contains(where:{$0.isWhitespace}) ? "@\"\(name)\" ":"@\(name) "
            return ComposerChoice(title:name,detail:$0["path"] ?? "",insertion:token)
        }
        return Array(result.prefix(7))
    }
}

struct ComposerSubmission {
    var text:String
    var project:[String:String]?
    var mode:String
    static func parse(_ value:String,projects:[[String:String]],selected:[String:String]?,taskMode:Bool)->ComposerSubmission {
        var remaining=value.trimmingCharacters(in:.whitespacesAndNewlines)
        var tokens:[String]=[];var project=selected;var mode=taskMode ? "task":"chat"
        while !remaining.isEmpty {
            if let regex=try? NSRegularExpression(pattern:"^/(委托|task|delegate|小黑板|黑板|board|聊天|chat)(?=\\s|[:：]|$)[\\s:：]*",options:.caseInsensitive),
               let match=regex.firstMatch(in:remaining,range:NSRange(location:0,length:(remaining as NSString).length)) {
                let command=(remaining as NSString).substring(with:match.range(at:1)).lowercased()
                if ["聊天","chat"].contains(command) { project=nil;mode="chat" }
                if ["委托","task","delegate"].contains(command) { mode="task" }
                tokens.append("/"+command);remaining=(remaining as NSString).substring(from:match.range.length);continue
            }
            var found=false
            for item in projects.sorted(by:{($0["name"] ?? "").count > ($1["name"] ?? "").count}) {
                guard let name=item["name"] else { continue }
                for token in ["@\"\(name)\"","@\(name)"] {
                    guard remaining.hasPrefix(token) else { continue }
                    let rest=String(remaining.dropFirst(token.count))
                    guard rest.isEmpty || rest.first!.isWhitespace else { continue }
                    project=item;mode="task";remaining=rest.trimmingCharacters(in:.whitespacesAndNewlines);found=true;break
                }
                if found { break }
            }
            if !found { break }
        }
        return ComposerSubmission(text:(tokens+[remaining]).joined(separator:" ").trimmingCharacters(in:.whitespacesAndNewlines),project:project,mode:mode)
    }
}

/// Shares the composer's translucent surface, spacing and neutral selection treatment.
final class ComposerSuggestionRow: NSTableRowView {
    override func drawSelection(in dirtyRect:NSRect) {
        NSColor.labelColor.withAlphaComponent(0.08).setFill()
        NSBezierPath(roundedRect:bounds.insetBy(dx:2,dy:2),xRadius:10,yRadius:10).fill()
    }
    override var interiorBackgroundStyle:NSView.BackgroundStyle { .normal }
}
final class ComposerSuggestions: NSObject, NSTableViewDataSource, NSTableViewDelegate {
    let panel=NSPanel(contentRect:NSRect(x:0,y:0,width:360,height:200),styleMask:[.borderless,.nonactivatingPanel],backing:.buffered,defer:false)
    let table=NSTableView(), scroll=NSScrollView()
    var choices:[ComposerChoice]=[]
    var query:ComposerQuery?
    var onChoose:((ComposerChoice,ComposerQuery)->Void)?
    override init() {
        super.init()
        panel.title="洛琪希 · 输入选项"
        panel.isReleasedWhenClosed=false;panel.level = .floating;panel.hasShadow=false
        panel.isOpaque=false;panel.backgroundColor = .clear;panel.hidesOnDeactivate=false
        panel.collectionBehavior=[.canJoinAllSpaces,.fullScreenAuxiliary]
        panel.contentView=CompanionGlass(frame:panel.contentView!.bounds)
        let column=NSTableColumn(identifier:NSUserInterfaceItemIdentifier("choice"));column.width=324;table.addTableColumn(column)
        table.style = .plain;table.usesAutomaticRowHeights=false
        table.headerView=nil;table.rowHeight=48;table.intercellSpacing = .zero
        table.backgroundColor = .clear;table.dataSource=self;table.delegate=self;table.focusRingType = .none
        table.target=self;table.action=#selector(selectChoice);table.allowsEmptySelection=false
        table.setAccessibilityLabel("项目与方式选项，方向键选择，Tab 或回车确认，Escape 关闭")
        scroll.documentView=table;scroll.hasVerticalScroller=true;scroll.autohidesScrollers=true;scroll.scrollerStyle = .overlay
        scroll.drawsBackground=false;scroll.borderType = .noBorder
        scroll.translatesAutoresizingMaskIntoConstraints=false;panel.contentView!.addSubview(scroll)
        NSLayoutConstraint.activate([
            scroll.leadingAnchor.constraint(equalTo:panel.contentView!.leadingAnchor,constant:16),
            scroll.trailingAnchor.constraint(equalTo:panel.contentView!.trailingAnchor,constant:-16),
            scroll.topAnchor.constraint(equalTo:panel.contentView!.topAnchor,constant:9),
            scroll.bottomAnchor.constraint(equalTo:panel.contentView!.bottomAnchor,constant:-9)])
    }
    func show(_ values:[ComposerChoice],query:ComposerQuery,anchor:NSRect) {
        self.query=query;choices=values
        guard !choices.isEmpty else { hide();return }
        let height=CGFloat(min(5,values.count))*48+18, width=anchor.width
        let screen=NSScreen.screens.first{$0.visibleFrame.intersects(anchor)}?.visibleFrame ?? anchor
        let y=anchor.maxY+6+height<=screen.maxY ? anchor.maxY+6:max(screen.minY,anchor.minY-height-6)
        panel.setFrame(NSRect(x:max(screen.minX,min(anchor.minX,screen.maxX-width)),y:y,width:width,height:height),display:true)
        scroll.hasVerticalScroller=values.count>5
        panel.contentView?.layoutSubtreeIfNeeded();scroll.tile()
        let contentWidth=scroll.contentSize.width
        table.tableColumns.first?.width=contentWidth
        table.frame=NSRect(x:0,y:0,width:contentWidth,height:CGFloat(values.count)*48)
        table.reloadData();table.selectRowIndexes(IndexSet(integer:0),byExtendingSelection:false);panel.orderFrontRegardless()
    }
    func hide() { panel.orderOut(nil);query=nil }
    func move(_ delta:Int) {
        guard !choices.isEmpty else{return}
        let next=max(0,min(choices.count-1,table.selectedRow+delta))
        table.selectRowIndexes(IndexSet(integer:next),byExtendingSelection:false);table.scrollRowToVisible(next)
    }
    @objc func selectChoice() {
        guard let query=query,choices.indices.contains(table.selectedRow) else{return}
        let choice=choices[table.selectedRow]
        guard !choice.insertion.isEmpty else { return }
        hide();onChoose?(choice,query)
    }
    func numberOfRows(in tableView:NSTableView)->Int { choices.count }
    func tableView(_ tableView:NSTableView,rowViewForRow row:Int)->NSTableRowView? { ComposerSuggestionRow() }
    func tableView(_ tableView:NSTableView,viewFor tableColumn:NSTableColumn?,row:Int)->NSView? {
        let item=choices[row],width=tableColumn?.width ?? 324
        let view=NSTableCellView(frame:NSRect(x:0,y:0,width:width,height:48))
        let symbol=query?.trigger=="@" ? "folder" : item.title=="委托" ? "arrow.up.right" : item.title=="小黑板" ? "rectangle.on.rectangle" : "bubble.left"
        let icon=NSImageView(frame:NSRect(x:10,y:15,width:18,height:18))
        icon.image=NSImage(systemSymbolName:symbol,accessibilityDescription:nil);icon.contentTintColor = .secondaryLabelColor
        let label=NSTextField(labelWithString:item.title);label.font = .systemFont(ofSize:14,weight:.medium)
        label.frame=NSRect(x:40,y:25,width:width-54,height:18);label.lineBreakMode = .byTruncatingTail
        let detail=NSTextField(labelWithString:item.detail.replacingOccurrences(of:NSHomeDirectory(),with:"~"))
        detail.font = .systemFont(ofSize:11);detail.textColor = .secondaryLabelColor;detail.lineBreakMode = .byTruncatingMiddle
        detail.frame=NSRect(x:40,y:7,width:width-54,height:15)
        view.addSubview(icon);view.addSubview(label);view.addSubview(detail);view.textField=label
        return view
    }
}

extension VoiceController {
    func controlTextDidChange(_ notification:Notification) { updateSuggestions() }
    func updateSuggestions() {
        guard composerShown,window.firstResponder === composerEditor,!composerEditor.hasMarkedText(),
              let query=ComposerQuery.at(composerEditor.string,cursor:composerEditor.selectedRange().location) else { suggestions.hide();return }
        var values=query.choices(projects)
        if query.trigger=="@" && values.isEmpty {
            values=[ComposerChoice(title:projectsLoaded ? "没有匹配的项目":"项目正在加载…",detail:"载入后可选择；也可右键刷新项目",insertion:"")]
        }
        suggestions.show(values,query:query,anchor:window.frame)
    }
    func commandKey(_ event:NSEvent)->Bool {
        guard !composerEditor.hasMarkedText() else{return false}
        if event.keyCode==48 && !suggestions.panel.isVisible { updateSuggestions();return true }
        guard suggestions.panel.isVisible else{return false}
        switch event.keyCode {
        case 125:suggestions.move(1)
        case 126:suggestions.move(-1)
        case 36,48:suggestions.selectChoice()
        case 53:suggestions.hide()
        default:return false
        }
        return true
    }
    func chooseSuggestion(_ choice:ComposerChoice,query:ComposerQuery) {
        guard NSMaxRange(query.range)<=(composerEditor.string as NSString).length else{return}
        composerEditor.insertText(choice.insertion,replacementRange:query.range)
        suggestions.hide();window.makeKeyAndOrderFront(nil);window.makeFirstResponder(composerEditor)
    }
}
