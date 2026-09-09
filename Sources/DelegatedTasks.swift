import Cocoa

func taskStatusLabel(_ value: String) -> String {
    ["queued":"排队中", "starting":"正在连接", "running":"执行中", "waiting":"需要你处理",
     "completed":"本轮已结束", "failed":"未完成", "cancelled":"已取消", "interrupted":"已中断"][value] ?? value
}

final class DelegatedTasksController: NSObject, NSTableViewDataSource, NSTableViewDelegate {
    let window = NSWindow(contentRect: NSRect(x:0,y:0,width:760,height:620), styleMask:[.titled,.closable,.resizable],backing:.buffered,defer:false)
    let table = NSTableView()
    let detail = NSTextView()
    let status = NSTextField(wrappingLabelWithString: "")
    let approve = NSButton(title:"允许本次",target:nil,action:nil)
    let deny = NSButton(title:"拒绝",target:nil,action:nil)
    let resumeButton = NSButton(title:"继续任务",target:nil,action:nil)
    let cancelButton = NSButton(title:"取消任务",target:nil,action:nil)
    let questionStack = NSStackView()
    var questionFields: [(String, NSTextField)] = []
    var tasks: [[String:Any]] = []
    var command: (([String:Any]) -> Void)?
    var onExplain: ((String)->Void)?
    var selectedID: String?
    var renderedToken: String?
    override init() {
        super.init()
        window.title="洛琪希 · 委托任务"; window.isReleasedWhenClosed=false; window.minSize=NSSize(width:660,height:520);window.center()
        window.contentView=SettingsBackground(frame:window.contentView!.bounds)
        let column=NSTableColumn(identifier:NSUserInterfaceItemIdentifier("task")); column.title="委托任务";column.width=720;table.addTableColumn(column)
        table.frame=NSRect(x:0,y:0,width:720,height:140);table.autoresizingMask=[.width]
        table.columnAutoresizingStyle = .uniformColumnAutoresizingStyle;table.usesAlternatingRowBackgroundColors=true
        table.headerView=nil;table.rowHeight=30;table.dataSource=self;table.delegate=self
        table.setAccessibilityLabel("委托任务列表")
        let list=NSScrollView();list.hasVerticalScroller=true;list.documentView=table
        list.heightAnchor.constraint(equalToConstant:140).isActive=true
        detail.isEditable=false; detail.isSelectable=true;detail.isRichText=false;detail.font = .systemFont(ofSize:13)
        detail.textContainerInset=NSSize(width:10,height:10);detail.autoresizingMask=[.width];detail.textContainer?.widthTracksTextView=true
        detail.setAccessibilityLabel("任务结果与授权详情")
        let scroll=NSScrollView();scroll.hasVerticalScroller=true;scroll.documentView=detail;scroll.borderType = .bezelBorder
        scroll.heightAnchor.constraint(greaterThanOrEqualToConstant:160).isActive=true
        questionStack.orientation = .vertical;questionStack.alignment = .leading;questionStack.spacing=6
        for (button, selector) in [(approve,#selector(acceptRequest)),(deny,#selector(declineRequest)),(resumeButton,#selector(resumeTask)),(cancelButton,#selector(cancelTask))] {
            button.target=self;button.action=selector;button.bezelStyle = .rounded
        }
        let reveal=NSButton(title:"打开工作目录",target:self,action:#selector(openFolder));reveal.bezelStyle = .rounded
        let explain=NSButton(title:"小黑板讲解",target:self,action:#selector(explainResult));explain.bezelStyle = .rounded
        let buttons=NSStackView(views:[cancelButton,resumeButton,reveal,explain,NSView(),deny,approve]);buttons.spacing=8
        status.font = .systemFont(ofSize:12);status.textColor = .secondaryLabelColor
        let stack=NSStackView(views:[list,status,scroll,questionStack,buttons]);stack.orientation = .vertical;stack.alignment = .leading;stack.spacing=12;stack.translatesAutoresizingMaskIntoConstraints=false
        window.contentView!.addSubview(stack)
        NSLayoutConstraint.activate([stack.leadingAnchor.constraint(equalTo:window.contentView!.leadingAnchor,constant:20),
            stack.trailingAnchor.constraint(equalTo:window.contentView!.trailingAnchor,constant:-20),
            stack.topAnchor.constraint(equalTo:window.contentView!.topAnchor,constant:20),
            stack.bottomAnchor.constraint(equalTo:window.contentView!.bottomAnchor,constant:-20)])
        for view in stack.arrangedSubviews { view.widthAnchor.constraint(equalTo:stack.widthAnchor).isActive=true }
        render()
    }
    @objc func explainResult() {
        if let job=selected,let ident=job["id"] as? String,!(job["result"] as? String ?? "").isEmpty { onExplain?(ident) }
    }
    func numberOfRows(in tableView:NSTableView)->Int { tasks.count }
    func tableView(_ tableView:NSTableView,viewFor tableColumn:NSTableColumn?,row:Int)->NSView? {
        let job=tasks[row]
        return NSTextField(labelWithString:"\(taskStatusLabel(job["status"] as? String ?? "")) · \(job["title"] as? String ?? "任务")")
    }
    func tableViewSelectionDidChange(_ notification:Notification) {
        if tasks.indices.contains(table.selectedRow) { selectedID=tasks[table.selectedRow]["id"] as? String }
        render()
    }
    var selected: [String:Any]? { tasks.first { $0["id"] as? String == selectedID } }
    func update(_ values:[[String:Any]]) {
        tasks=values.sorted { ($0["created"] as? Double ?? 0) > ($1["created"] as? Double ?? 0) }
        if selectedID == nil { selectedID=tasks.first?["id"] as? String }
        table.reloadData()
        if let index=tasks.firstIndex(where:{$0["id"] as? String == selectedID}) { table.selectRowIndexes(IndexSet(integer:index),byExtendingSelection:false) }
        render()
    }
    func show(_ ident:String?=nil) {
        if let ident=ident { selectedID=ident }
        update(tasks);window.makeKeyAndOrderFront(nil);NSApp.activate(ignoringOtherApps:true)
    }
    func render() {
        guard let job=selected else { detail.string="右键选择项目，或在聊天中输入“委托：……”，即可开始。";approve.isHidden=true;deny.isHidden=true;return }
        let state=job["status"] as? String ?? ""
        status.stringValue="\(taskStatusLabel(state)) · \(job["cwd"] as? String ?? "")"
        var text="需求：\n\(job["prompt"] as? String ?? "")\n\n\(job["result"] as? String ?? "")"
        if let thread=job["thread_id"] as? String { text += "\n\nCodex 任务 ID：\(thread)" }
        let request=job["request"] as? [String:Any]
        let token=request?["token"] as? String
        if let warning=job["capability_warning"] as? String { text += "\n\n" + warning }
        if let request=request, let details=request["details"],let data=try? JSONSerialization.data(withJSONObject:details,options:[.prettyPrinted,.sortedKeys]),let pretty=String(data:data,encoding:.utf8) {
            text += "\n\n需要你处理的请求（请查看完整操作）：\n" + pretty
        }
        detail.string=text
        approve.isHidden=request == nil;deny.isHidden=request == nil
        approve.title=(request?["method"] as? String == "item/tool/requestUserInput") ? "提交回答":"允许本次"
        resumeButton.isEnabled=["failed","cancelled","interrupted"].contains(state)
        cancelButton.isEnabled=["queued","starting","running","waiting"].contains(state)
        if token != renderedToken {
            renderedToken=token
            for view in questionStack.arrangedSubviews { questionStack.removeArrangedSubview(view);view.removeFromSuperview() }
            questionFields=[]
            if let details=request?["details"] as? [String:Any],let questions=details["questions"] as? [[String:Any]] {
                for question in questions {
                    let label=NSTextField(wrappingLabelWithString:question["question"] as? String ?? "请补充信息")
                    let field=NSTextField(string:"");field.placeholderString="输入回答"
                    field.setAccessibilityLabel(question["question"] as? String ?? "回答")
                    questionStack.addArrangedSubview(label);questionStack.addArrangedSubview(field)
                    field.widthAnchor.constraint(equalTo:questionStack.widthAnchor).isActive=true
                    questionFields.append((question["id"] as? String ?? "",field))
                }
            }
        }
    }
    @objc func acceptRequest() {
        guard let token=(selected?["request"] as? [String:Any])?["token"] as? String else{return}
        if !questionFields.isEmpty && questionFields.contains(where:{$0.1.stringValue.trimmingCharacters(in:.whitespacesAndNewlines).isEmpty}) {
            status.stringValue="请回答问题后提交，或取消这个任务。";return
        }
        command?(["type":"task_response","token":token,"accepted":true,"answers":Dictionary(uniqueKeysWithValues:questionFields.map{($0.0,$0.1.stringValue)})])
    }
    @objc func declineRequest() {
        guard let request=selected?["request"] as? [String:Any],let token=request["token"] as? String else{return}
        if request["method"] as? String == "item/tool/requestUserInput" { cancelTask();return }
        command?(["type":"task_response","token":token,"accepted":false])
    }
    @objc func cancelTask() { if let id=selectedID { command?(["type":"task_cancel","task_id":id]) } }
    @objc func resumeTask() { if let id=selectedID { command?(["type":"task_resume","task_id":id]) } }
    @objc func openFolder() { if let cwd=selected?["cwd"] as? String { NSWorkspace.shared.open(URL(fileURLWithPath:cwd)) } }
}
