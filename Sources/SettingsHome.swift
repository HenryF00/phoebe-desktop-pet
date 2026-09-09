import Cocoa

/// One entry point, with existing editors embedded rather than opening more settings windows.
final class SettingsHomeController: NSObject, NSWindowDelegate {
    let window=NSWindow(contentRect:NSRect(x:0,y:0,width:680,height:620),styleMask:[.titled,.closable],backing:.buffered,defer:false)
    let tabs=NSTabView()
    let chat: ChatSettingsController
    let memory: CompanionMemoryController
    weak var voice: VoiceController?
    let size=NSPopUpButton(frame:.zero,pullsDown:false)
    let automatic=NSButton(checkboxWithTitle:"自动互动与视线跟随",target:nil,action:nil)
    let follow=NSButton(checkboxWithTitle:"跟随 Codex 任务动作",target:nil,action:nil)
    let pause=NSButton(checkboxWithTitle:"暂停角色动画",target:nil,action:nil)
    let notifications=NSButton(checkboxWithTitle:"Codex 事件语音提醒",target:nil,action:nil)
    let delivery=NSPopUpButton(frame:.zero,pullsDown:false)
    let narration=NSButton(checkboxWithTitle:"打开小黑板时，用语音讲解重点",target:nil,action:nil)
    let taskHint=NSTextField(wrappingLabelWithString:"")
    init(voice:VoiceController,chat:ChatSettingsController,memory:CompanionMemoryController) {
        self.voice=voice;self.chat=chat;self.memory=memory;super.init()
        window.title="洛琪希 · 全部设置";window.isReleasedWhenClosed=false;window.delegate=self;window.center()
        window.contentView=SettingsBackground(frame:window.contentView!.bounds)
        tabs.translatesAutoresizingMaskIntoConstraints=false;window.contentView!.addSubview(tabs)
        NSLayoutConstraint.activate([tabs.leadingAnchor.constraint(equalTo:window.contentView!.leadingAnchor,constant:16),
            tabs.trailingAnchor.constraint(equalTo:window.contentView!.trailingAnchor,constant:-16),
            tabs.topAnchor.constraint(equalTo:window.contentView!.topAnchor,constant:16),
            tabs.bottomAnchor.constraint(equalTo:window.contentView!.bottomAnchor,constant:-16)])
        size.addItems(withTitles:["300 点","400 点","500 点","560 点"])
        size.target=self;size.action=#selector(saveGeneral)
        for toggle in [automatic,follow,pause,notifications,narration] { toggle.target=self;toggle.action=#selector(saveGeneral) }
        delivery.addItems(withTitles:["完成后自动打开小黑板并通报","只亮蓝点，点击后查看答案"])
        delivery.target=self;delivery.action=#selector(saveGeneral)
        let reset=NSButton(title:"将洛琪希移回屏幕右下角",target:self,action:#selector(resetPosition));reset.bezelStyle = .rounded
        let history=NSButton(title:"查看聊天记录…",target:self,action:#selector(showHistory));history.bezelStyle = .rounded
        let general=page("陪伴与显示",description:"单击聊天，双击进入这里。修改立即生效。",views:[chat.row("角色大小",size),automatic,follow,pause,reset,history])
        add("通用",general)
        let chatView=chat.window.contentView!;chat.window.contentView=nil;add("聊天与声音",chatView)
        taskHint.font = .systemFont(ofSize:13);taskHint.textColor = .secondaryLabelColor
        let taskList=NSButton(title:"管理委托任务…",target:self,action:#selector(showTasks));taskList.bezelStyle = .rounded
        let task=page("让洛琪希讲重点",description:"闲聊和简单任务用字幕气泡；需要解释或可视化时用小黑板展示要点、数据和来源，也可以输入 /小黑板 主动调用。语音解释含义，不逐项照读。讲解整理使用 Codex 订阅，声音在本机合成。",views:[delivery,narration,taskHint,notifications,taskList])
        add("任务与小黑板",task)
        let memoryView=memory.window.contentView!;memory.window.contentView=nil;add("记忆",memoryView)
        chat.onMemory={ [weak self] in self?.tabs.selectTabViewItem(at:3) }
    }
    func page(_ title:String,description:String,views:[NSView])->NSView {
        let page=SettingsBackground()
        let heading=NSTextField(labelWithString:title);heading.font = .systemFont(ofSize:23,weight:.semibold)
        let hint=NSTextField(wrappingLabelWithString:description);hint.font = .systemFont(ofSize:13);hint.textColor = .secondaryLabelColor
        let stack=NSStackView(views:[heading,hint]+views+[NSView()]);stack.orientation = .vertical;stack.alignment = .leading;stack.spacing=22
        stack.translatesAutoresizingMaskIntoConstraints=false;page.addSubview(stack)
        NSLayoutConstraint.activate([stack.leadingAnchor.constraint(equalTo:page.leadingAnchor,constant:24),stack.trailingAnchor.constraint(equalTo:page.trailingAnchor,constant:-24),stack.topAnchor.constraint(equalTo:page.topAnchor,constant:24),stack.bottomAnchor.constraint(equalTo:page.bottomAnchor,constant:-24)])
        for view in stack.arrangedSubviews { view.widthAnchor.constraint(equalTo:stack.widthAnchor).isActive=true }
        return page
    }
    func add(_ title:String,_ view:NSView) {
        let tab=NSTabViewItem(identifier:title);tab.label=title;tab.view=view;tabs.addTabViewItem(tab)
    }
    func show() {
        guard let voice=voice else { return }
        let values=voice.petSettingsRead?() ?? (560,true,true,false)
        size.selectItem(at:[300,400,500,560].firstIndex(of:Int(values.0)) ?? 3)
        automatic.state=values.1 ? .on:.off;follow.state=values.2 ? .on:.off;pause.state=values.3 ? .on:.off
        notifications.state=voice.notificationEnabled ? .on:.off
        delivery.selectItem(at:voice.preferences.taskDelivery == "manual" ? 1:0)
        narration.state=voice.preferences.boardSpeech ? .on:.off
        chat.load(voice.preferences);memory.reload();updateHint()
        window.makeKeyAndOrderFront(nil);NSApp.activate(ignoringOtherApps:true)
    }
    func windowWillClose(_ notification:Notification) { chat.key.stringValue="" }
    func updateHint() {
        taskHint.stringValue = delivery.indexOfSelectedItem == 1
            ? "蓝色转圈：正在执行。蓝色暂停：需要处理。蓝点：有未读结果。点击蓝点打开小黑板；查看后清除。"
            : "任务完成后，等当前对话与语音结束再打开小黑板；不会打断正在播放的回答。蓝点保留到结果被展示。"
    }
    @objc func saveGeneral() {
        guard let voice=voice else { return }
        voice.petSettingsApply?(Double([300,400,500,560][max(0,size.indexOfSelectedItem)]),automatic.state == .on,follow.state == .on,pause.state == .on)
        voice.notificationEnabled=notifications.state == .on
        voice.sendCommand(["type":"notifications","enabled":voice.notificationEnabled])
        voice.preferences.taskDelivery=delivery.indexOfSelectedItem == 1 ? "manual":"auto"
        voice.preferences.boardSpeech=narration.state == .on;voice.preferences.save()
        voice.sendCommand(["type":"configure","settings":voice.preferences.payload])
        if voice.preferences.taskDelivery == "manual" { voice.automaticBoards.removeAll();voice.autoAwaitingBoards.removeAll();voice.pendingBriefTasks.removeAll() }
        if !voice.preferences.boardSpeech { voice.stopBoardSpeech() }
        voice.onMenuChange?();updateHint()
    }
    @objc func resetPosition() { voice?.petResetPosition?() }
    @objc func showHistory() { voice?.showHistory() }
    @objc func showTasks() { voice?.showTasks() }
}
