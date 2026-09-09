import Cocoa
import AVFoundation

final class TalkButton: NSButton {
    var isPointerTracking = false
    var pressed: (() -> Void)?
    var released: (() -> Void)?
    override func mouseDown(with event: NSEvent) {
        guard isEnabled else { return }
        isPointerTracking = true
        defer { isPointerTracking = false }
        pressed?(); super.mouseDown(with: event); released?()
    }
}

/// Owns one local worker and one playback queue for both chat and announcements.
final class VoiceController: NSObject, AVAudioPlayerDelegate, NSWindowDelegate, NSTextFieldDelegate {
    let root: URL
    var window: NSWindow!
    var bubble: CompanionPanel!
    let subtitle = HistoryTextField(wrappingLabelWithString: "你好，我在这里。想聊些什么？")
    var lecturePlacement:((NSWindow?)->Void)?
    var lectureMode=false
    var lecturePreviousComposer=false
    var lectureCurrentTurn=false
    var lectureDelegatedTask:String?
    var anchorProvider: (() -> (NSRect, NSRect)?)?
    var composerShown = false
    var bubbleShown = false
    var bubbleGlass = CompanionGlass()
    var textEntry: NSStackView!
    var preferences = ChatPreferences.load()
    var settingsHome: SettingsHomeController?
    var petSettingsRead: (() -> (Double,Bool,Bool,Bool))?
    var petSettingsApply: ((Double,Bool,Bool,Bool) -> Void)?
    var petResetPosition: (() -> Void)?
    var boardController: TeachingBoardController?
    var boards: [String:[String:Any]] = [:]
    var chatBoards: [String:[String:Any]] = [:]
    var requestedBoard: String?
    var automaticBoards: [String] = []
    var pendingBriefTasks:[String]=[]
    var autoAwaitingBoards = Set<String>()
    var boardSpeechRequest: String?
    var boardSpeechTask: String?
    var boardAudioQueue:[LessonAudio]=[]
    var boardAudioDone=false
    var boardSpeechFailure:String?
    var currentLessonAudio:LessonAudio?
    var boardAudio: (Data,String)? {
        get { boardAudioQueue.first.map { ($0.data,$0.caption) } }
        set { boardAudioQueue=newValue.map { [LessonAudio(data:$0.0,caption:$0.1,focus:"takeaway",gesture:"explain",index:0)] } ?? [] }
    }
    var activeBoardSpeech = false
    var memoryController: CompanionMemoryController?
    var settingsController: ChatSettingsController?
    let userCaption = HistoryTextField(wrappingLabelWithString: "")
    var userSection: NSStackView!
    var displayedUser = ""
    var pendingReplyMessage = ""
    var noticeMessage = ""
    var historyController: ChatHistoryController?
    let persistsHistory: Bool
    var activeSubtitle = ""
    var activeIsNotice = false
    let transcript = NSTextView()
    let input = NSTextField(string: "")
    let composerEditor = ComposerEditor()
    let suggestions=ComposerSuggestions()
    var selectedProject: [String:String]?
    var taskMode = false
    var attachmentPaths: [String] = []
    var projects: [[String:String]] = []
    var projectsLoaded = false
    var delegatedTasks: [[String:Any]] = []
    var tasksController: DelegatedTasksController?
    var notifiedTasks = Set<String>()
    var onProjectMenuChange: (() -> Void)?
    let phase = NSTextField(labelWithString: "按住说话，松开后洛琪希会用日语回答。")
    let connection = NSTextField(labelWithString: "Codex · 发送消息时连接")
    let hookLabel = NSTextField(labelWithString: "任务播报 · 等待 Codex 事件")
    let talk = TalkButton(title: "按住说话", target: nil, action: nil)
    let recordToggle = NSButton(title: "点击录音", target: nil, action: nil)
    let notifyToggle = NSButton(checkboxWithTitle: "任务语音播报", target: nil, action: nil)
    var worker: Process?
    var workerInput: FileHandle?
    var workerOutput: FileHandle?
    var receiveBuffer = Data()
    let outputQueue = DispatchQueue(label: "roxy.voice.protocol")
    let inputQueue = DispatchQueue(label: "roxy.voice.commands")
    var recorder: AVAudioRecorder?
    var recordingURL: URL?
    var recordingTimer: Timer?
    var recordingBegan = Date()
    let streamPlayer = StreamingAudioPlayer()
    var player: AVAudioPlayer?
    var clips: [(Data, Bool, String)] = []
    var currentID: String?
    var currentUser = ""
    var currentAnswer = ""
    var replyInsertIndex = 0
    var history: [[String: String]] = []
    var streamSpeaking = false
    var replyExpression = "calm"
    var reactionUntil = 0.0
    var reactionPose: String?
    var petPose: String? {
        if recorder != nil { return "chat-listening" }
        if activeBoardSpeech {
            let right=(boardController?.window.frame.midX ?? 0) > (anchorProvider?()?.0.midX ?? .greatestFiniteMagnitude)
            return teachingPose(gesture:currentLessonAudio?.gesture ?? "explain",elapsed:player?.currentTime ?? 0,boardOnRight:right)
        }
        if player != nil { return "chat-speaking" }
        if streamSpeaking { return expressionPose(replyExpression) }
        if ProcessInfo.processInfo.systemUptime < reactionUntil { return reactionPose }
        if generating || requestedBoard != nil { return "chat-thinking" }
        if boardSpeechRequest != nil { return "teaching-explain" }
        return nil
    }
    func expressionPose(_ expression: String) -> String {
        switch expression {
        case "shy": return "chat-shy"
        case "nod": return "chat-nod"
        case "sad": return "failed"
        case "thoughtful": return "chat-thinking"
        default: return "chat-speaking"
        }
    }
    func holdReaction(_ pose: String, seconds: Double = 1.2) {
        reactionPose = pose; reactionUntil = ProcessInfo.processInfo.systemUptime + seconds
    }
    var generating = false
    var shuttingDown = false
    var notificationEnabled: Bool {
        get { UserDefaults.standard.object(forKey: "voiceNotifications") as? Bool ?? true }
        set { UserDefaults.standard.set(newValue, forKey: "voiceNotifications") }
    }
    var onMenuChange: (() -> Void)?

    init(root: URL, startWorker: Bool = true, persistHistory: Bool? = nil) {
        self.root = root; self.persistsHistory = persistHistory ?? startWorker
        super.init()
        loadTranscript(); buildWindow()
        composerEditor.isFieldEditor = true
        composerEditor.selectionChanged = { [weak self] in self?.updateSuggestions() }
        composerEditor.commandKey = { [weak self] event in self?.commandKey(event) ?? false }
        suggestions.onChoose = { [weak self] choice,query in self?.chooseSuggestion(choice,query:query) }
        composerEditor.pasteAttachment = { [weak self] in self?.pasteImage() ?? false }
        composerEditor.contextMenu = { [weak self] in self?.composerMenu() ?? NSMenu() }
        streamPlayer.onSegmentBegan = { [weak self] expression in
            self?.replyExpression = expression; self?.streamSpeaking = true
        }
        streamPlayer.onBegan = { [weak self] text in
            guard let self = self else { return }
            self.activeSubtitle = text; self.activeIsNotice = false
            // Playback advances independently; it must never replace generated reply text.
            self.phase.stringValue = "洛琪希正在说话…"
        }
        streamPlayer.onIdle = { [weak self] in
            guard let self = self else { return }
            self.phase.stringValue = self.generating ? "洛琪希还在组织下一句话…" : "我在听，随时可以继续。"
            self.streamSpeaking = false; self.holdReaction(self.expressionPose(self.replyExpression))
            self.playNext(); self.showReplyWhenIdle()
        }
        streamPlayer.onError = { [weak self] in
            self?.streamSpeaking = false; self?.holdReaction("failed")
            self?.phase.stringValue = "音频播放失败，请检查声音输出。"
        }
        if startWorker { launchWorker() }
    }
    func label(_ value: String, size: CGFloat, weight: NSFont.Weight = .regular) -> NSTextField {
        let field = NSTextField(labelWithString: value)
        field.font = .systemFont(ofSize: size, weight: weight)
        return field
    }
    func button(_ title: String, _ action: Selector) -> NSButton {
        let b = NSButton(title: title, target: self, action: action)
        b.bezelStyle = .rounded; b.controlSize = .large
        return b
    }
    func makePanel(size: NSSize, keyboard: Bool) -> CompanionPanel {
        let panel = CompanionPanel(contentRect: NSRect(origin: .zero, size: size),
            styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        panel.acceptsKeyboard = keyboard; panel.isOpaque = false; panel.backgroundColor = .clear
        panel.hasShadow = false; panel.level = .floating; panel.hidesOnDeactivate = false
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        panel.isReleasedWhenClosed = false
        return panel
    }
    func iconButton(_ symbol: String, label: String, action: Selector) -> NSButton {
        let b = button("", action)
        b.image = NSImage(systemSymbolName: symbol, accessibilityDescription: label)
        b.imagePosition = .imageOnly; b.toolTip = label; b.setAccessibilityLabel(label)
        b.widthAnchor.constraint(equalToConstant: 32).isActive = true
        b.heightAnchor.constraint(equalToConstant: 32).isActive = true
        return b
    }
    func buildWindow() {
        window = makePanel(size: NSSize(width: 360, height: 54), keyboard: true)
        window.title = "洛琪希 · 聊天输入"; window.delegate = self
        let glass = CompanionGlass(frame: NSRect(x: 0, y: 0, width: 360, height: 54))
        window.contentView = glass
        input.placeholderString = nil
        (input.cell as? NSTextFieldCell)?.sendsActionOnEndEditing=false
        input.font = .systemFont(ofSize: 14); input.target = self; input.action = #selector(sendText)
        input.isBordered = false; input.drawsBackground = false; input.focusRingType = .none
        input.delegate = self
        input.setAccessibilityLabel("输入给洛琪希的消息，回车发送")
        input.heightAnchor.constraint(equalToConstant: 30).isActive = true
        talk.title = ""; talk.image = NSImage(systemSymbolName:"mic",accessibilityDescription:"按住说话")
        talk.imagePosition = .imageOnly; talk.isBordered = false; talk.focusRingType = .none
        talk.toolTip = "按住说话，松开发送；键盘空格可开始或结束录音"
        talk.pressed = { [weak self] in self?.beginRecording() }
        talk.released = { [weak self] in self?.finishRecording(send: true) }
        talk.setAccessibilityLabel("按住说话，松开发送")
        talk.target = self; talk.action = #selector(talkAction)
        talk.widthAnchor.constraint(equalToConstant:32).isActive = true
        talk.heightAnchor.constraint(equalToConstant:32).isActive = true
        textEntry = NSStackView(views:[input,talk]); textEntry.orientation = .horizontal; textEntry.spacing = 8
        textEntry.translatesAutoresizingMaskIntoConstraints = false; glass.addSubview(textEntry)
        NSLayoutConstraint.activate([
            textEntry.leadingAnchor.constraint(equalTo:glass.leadingAnchor,constant:22),
            textEntry.trailingAnchor.constraint(equalTo:glass.trailingAnchor,constant:-18),
            textEntry.centerYAnchor.constraint(equalTo:glass.centerYAnchor)])
        bubble = makePanel(size: NSSize(width: 360, height: 126), keyboard: false)
        bubble.title = "洛琪希 · 对话"
        bubbleGlass = CompanionGlass(frame: NSRect(x: 0, y: 0, width: 360, height: 126)); bubbleGlass.tail = true
        bubble.contentView = bubbleGlass
        bubbleGlass.onDoubleClick = { [weak self] in self?.showHistory() }
        bubbleGlass.toolTip = "双击查看聊天记录"
        let name = HistoryTextField(labelWithString:"洛琪希")
        name.font = .systemFont(ofSize:11,weight:.semibold); name.textColor = .secondaryLabelColor
        name.onDoubleClick = { [weak self] in self?.showHistory() }
        let userName = HistoryTextField(labelWithString:"你")
        userName.font = .systemFont(ofSize:11,weight:.semibold);userName.textColor = .secondaryLabelColor
        userName.onDoubleClick = { [weak self] in self?.showHistory() }
        userCaption.font = .systemFont(ofSize:14);userCaption.textColor = .secondaryLabelColor
        userCaption.maximumNumberOfLines=4;userCaption.lineBreakMode = .byWordWrapping
        userCaption.onDoubleClick = { [weak self] in self?.showHistory() }
        userCaption.setAccessibilityLabel("你本轮发送的文字或语音识别结果")
        userSection=NSStackView(views:[userName,userCaption]);userSection.orientation = .vertical
        userSection.alignment = .leading;userSection.spacing=5;userSection.isHidden=true
        userCaption.widthAnchor.constraint(equalToConstant:312).isActive=true
        subtitle.font = .systemFont(ofSize: 16); subtitle.textColor = .labelColor
        subtitle.isSelectable = false; subtitle.maximumNumberOfLines = 12
        subtitle.lineBreakMode = .byWordWrapping
        subtitle.onDoubleClick = { [weak self] in self?.showHistory() }
        subtitle.setAccessibilityLabel("洛琪希正在说的字幕，双击查看聊天记录")
        let lines = NSStackView(views: [userSection, name, subtitle]); lines.orientation = .vertical; lines.alignment = .leading; lines.spacing = 7
        lines.translatesAutoresizingMaskIntoConstraints = false; bubbleGlass.addSubview(lines)
        NSLayoutConstraint.activate([
            lines.leadingAnchor.constraint(equalTo: bubbleGlass.leadingAnchor, constant: 24),
            lines.trailingAnchor.constraint(equalTo: bubbleGlass.trailingAnchor, constant: -24),
            lines.topAnchor.constraint(equalTo: bubbleGlass.topAnchor, constant: 16),
            subtitle.widthAnchor.constraint(equalTo: lines.widthAnchor),
        ])
        notifyToggle.state = notificationEnabled ? .on : .off
        if transcript.string.isEmpty { append("洛琪希", "你好，我在这里。想聊些什么？") }
    }
    func show() {
        if lectureMode,let board=boardController,board.window.isVisible {
            board.captions.panel.makeKeyAndOrderFront(nil)
            board.captions.panel.makeFirstResponder(board.captions.input);return
        }
        composerShown = true; updateAnchor()
        window.makeKeyAndOrderFront(nil); NSApp.activate(ignoringOtherApps: true)
        window.makeFirstResponder(input)
    }
    func toggleChat() { if composerShown { hideChat() } else { show() } }
    @objc func hideChat() {
        suggestions.hide();stopAll(); composerShown = false; bubbleShown = false
        window.orderOut(nil); bubble.orderOut(nil)
    }
    func windowWillReturnFieldEditor(_ sender: NSWindow, to client: Any?) -> Any? {
        if let field = client as? NSTextField, field === input { return composerEditor }
        return nil
    }
    func pasteImage() -> Bool {
        let board = composerEditor.clipboard
        // Preserve regular copied text/URLs even when rich clipboard flavors are present.
        if board.string(forType: .string) != nil { return false }
        guard let image = NSImage(pasteboard:board), let tiff=image.tiffRepresentation,
              let bitmap=NSBitmapImageRep(data:tiff),let data=bitmap.representation(using:.png,properties:[:]) else { return false }
        guard attachmentPaths.count < 4, data.count <= 20*1024*1024 else { showSubtitle("最多粘贴四张截图，每张不超过 20 MB。");return true }
        do {
            let dir=root.appendingPathComponent("runtime/attachments")
            try FileManager.default.createDirectory(at:dir,withIntermediateDirectories:true)
            let url=dir.appendingPathComponent(UUID().uuidString+".png")
            guard FileManager.default.createFile(atPath:url.path,contents:data,attributes:[.posixPermissions:0o600]) else { throw CocoaError(.fileWriteUnknown) }
            attachmentPaths.append(url.path); taskMode=true; updateComposerContext()
            showSubtitle("已附上第 \(attachmentPaths.count) 张截图。输入要求后回车，交给 Codex 处理。")
        } catch { showSubtitle("截图未能保存，请重新粘贴。") }
        return true
    }
    func updateComposerContext() {
        let context=selectedProject?["name"] ?? (taskMode ? "Codex 任务":"")
        input.placeholderString=nil
        input.toolTip=(selectedProject?["path"] ?? context) + (attachmentPaths.isEmpty ? "":" · 已附 \(attachmentPaths.count) 张截图")
        onProjectMenuChange?()
    }
    func projectMenu() -> NSMenu {
        let menu=NSMenu()
        func item(_ title:String,_ selector:Selector,_ object:Any?=nil)->NSMenuItem {
            let item=NSMenuItem(title:title,action:selector,keyEquivalent:"");item.target=self;item.representedObject=object;return item
        }
        let chat=item("普通聊天",#selector(selectChatMode));chat.state = !taskMode && selectedProject == nil ? .on:.off;menu.addItem(chat)
        let task=item("联网分析 / 委托任务",#selector(selectTaskMode));task.state = taskMode && selectedProject == nil ? .on:.off;menu.addItem(task)
        menu.addItem(.separator())
        for project in projects {
            let row=item(project["name"] ?? "项目",#selector(selectProject(_:)),project)
            row.toolTip=project["path"];row.state = selectedProject == project ? .on:.off;menu.addItem(row)
        }
        if projects.isEmpty { let row=NSMenuItem(title:"尚未载入项目，请刷新",action:nil,keyEquivalent:"");menu.addItem(row) }
        menu.addItem(.separator());menu.addItem(item("刷新 Codex 项目",#selector(refreshProjects)))
        menu.addItem(item("查看委托任务…",#selector(showTasks)))
        return menu
    }
    func composerMenu() -> NSMenu {
        let menu=NSMenu()
        for (title,selector) in [("粘贴",#selector(pasteIntoComposer)),("复制",#selector(copyFromComposer)),("全选",#selector(selectComposerText))] {
            let item=NSMenuItem(title:title,action:selector,keyEquivalent:"");item.target=self;menu.addItem(item)
        }
        menu.addItem(.separator())
        let projects=NSMenuItem(title:"选择 Codex 项目",action:nil,keyEquivalent:"");projects.submenu=projectMenu();menu.addItem(projects)
        if !attachmentPaths.isEmpty { let clear=NSMenuItem(title:"移除已粘贴截图",action:#selector(clearAttachments),keyEquivalent:"");clear.target=self;menu.addItem(clear) }
        return menu
    }
    @objc func pasteIntoComposer() { if window.firstResponder !== composerEditor { show() };composerEditor.paste(nil) }
    @objc func copyFromComposer() { composerEditor.copy(nil) }
    @objc func selectComposerText() { composerEditor.selectAll(nil) }
    @objc func selectChatMode() { selectedProject=nil;taskMode=false;clearAttachments();updateComposerContext();show() }
    @objc func selectTaskMode() { selectedProject=nil;taskMode=true;updateComposerContext();show() }
    @objc func selectProject(_ item:NSMenuItem) { selectedProject=item.representedObject as? [String:String];taskMode=true;updateComposerContext();show() }
    @objc func clearAttachments() {
        for path in attachmentPaths { try? FileManager.default.removeItem(atPath:path) }
        attachmentPaths=[];updateComposerContext()
    }
    @objc func refreshProjects() { launchWorker();sendCommand(["type":"projects"]) }
    @objc func showTasks() {
        if tasksController == nil {
            tasksController=DelegatedTasksController()
            tasksController?.command = { [weak self] command in self?.sendCommand(command) }
            tasksController?.onExplain = { [weak self] ident in self?.openBoard(ident) }
        }
        tasksController?.update(delegatedTasks);tasksController?.show()
        launchWorker();sendCommand(["type":"task_list"])
    }
    func controlTextDidBeginEditing(_ notification: Notification) {
        if let editor = input.currentEditor() as? NSTextView {
            editor.focusRingType = .none; editor.drawsBackground = false
        }
    }
    func control(_ control: NSControl, textView: NSTextView, doCommandBy commandSelector: Selector) -> Bool {
        if commandSelector == #selector(NSResponder.cancelOperation(_:)) { stopAll(); return true }
        return false
    }
    @objc func showSettings() {
        stopAll()
        if settingsController == nil {
            let settings = ChatSettingsController()
            settings.onSave = { [weak self] value in
                guard let self = self else { return }
                self.stopAll(); self.preferences = value; self.boards.removeAll()
                self.sendCommand(["type":"configure", "settings":value.payload])
            }
            settings.onPreview = { [weak self] in
                self?.launchWorker(); self?.sendCommand(["type":"preview"])
            }
            settings.onMemory = { [weak self] in self?.showMemory() }
            settings.onReset = { [weak self] in self?.clearChat() }
            settingsController = settings
        }
        if memoryController == nil {
            memoryController=CompanionMemoryController(url:root.appendingPathComponent("runtime/companion-memory.json"))
        }
        if settingsHome == nil,let chat=settingsController,let memory=memoryController {
            settingsHome=SettingsHomeController(voice:self,chat:chat,memory:memory)
        }
        settingsHome?.show()
    }
    func showMemory() { showSettings();settingsHome?.tabs.selectTabViewItem(at:3) }
    @objc func talkAction() {
        // Mouse press/release is handled by TalkButton; keyboard activation toggles recording.
        if !talk.isPointerTracking { toggleRecording() }
    }
    func updateAnchor() {
        if lectureMode { return }
        guard let (anchor, screen) = anchorProvider?() else { return }
        let total = NSSize(width: 360, height: (bubbleShown ? bubble.frame.height + 8 : 0) + 54)
        let (frame, onLeft) = companionFrame(anchor: anchor, size: total, screen: screen)
        bubbleGlass.pointsRight = onLeft
        let inputFrame = NSRect(x: frame.minX, y: frame.minY, width: 360, height: 54)
        if window.frame != inputFrame { window.setFrame(inputFrame, display: true) }
        let subtitleFrame = NSRect(x: frame.minX, y: frame.minY + 62, width: 360, height: bubble.frame.height)
        if bubble.frame != subtitleFrame { bubble.setFrame(subtitleFrame, display: true) }
    }
    func showSubtitle(_ text: String, reveal: Bool = true) {
        guard !text.isEmpty else { return }
        subtitle.stringValue = text
        if lectureMode { bubble.orderOut(nil);return }
        let rect = (text as NSString).boundingRect(with: NSSize(width:312,height:2000),
            options:[.usesLineFragmentOrigin, .usesFontLeading], attributes:[.font:NSFont.systemFont(ofSize:16)])
        let userRect = (displayedUser as NSString).boundingRect(with:NSSize(width:312,height:2000),
            options:[.usesLineFragmentOrigin,.usesFontLeading],attributes:[.font:NSFont.systemFont(ofSize:14)])
        let userHeight:CGFloat = displayedUser.isEmpty ? 0 : min(72,ceil(userRect.height))+33
        bubble.setContentSize(NSSize(width:360, height:max(98,min(240,ceil(rect.height))+64+userHeight)))
        if reveal { bubbleShown = true }
        updateAnchor()
        if bubbleShown { bubble.orderFrontRegardless() }
    }
    func setDisplayedUser(_ text: String) {
        displayedUser=text;userCaption.stringValue=text;userSection.isHidden=text.isEmpty
        if lectureMode && lectureCurrentTurn { boardController?.captions.setUserText(text) }
        showSubtitle(subtitle.stringValue)
    }
    func renderReplyText() {
        // One owner for reply content, regardless of audio buffering or completion timing.
        var parts=[currentAnswer]
        if !pendingReplyMessage.isEmpty { parts.append(pendingReplyMessage) }
        if !noticeMessage.isEmpty { parts.append("提醒：" + noticeMessage) }
        let text=parts.filter { !$0.isEmpty }.joined(separator:"\n\n")
        if !text.isEmpty && subtitle.stringValue != text { showSubtitle(text) }
    }
    func showReplyWhenIdle() {
        guard !generating, player == nil, !streamPlayer.isBusy, clips.isEmpty else { return }
        renderReplyText()
    }
    @objc func showHistory() {
        if historyController == nil { historyController=ChatHistoryController(transcript:transcript) }
        historyController?.show()
    }
    var historyURL: URL { root.appendingPathComponent("runtime/chat-history.rtf") }
    func loadTranscript() {
        guard persistsHistory, let data=try? Data(contentsOf:historyURL),
              let text=try? NSAttributedString(data:data,options:[.documentType:NSAttributedString.DocumentType.rtf],documentAttributes:nil) else { return }
        transcript.textStorage?.setAttributedString(text)
        transcript.textStorage?.addAttribute(.foregroundColor,value:NSColor.labelColor,range:NSRange(location:0,length:transcript.textStorage?.length ?? 0))
    }
    func saveTranscript() {
        guard persistsHistory,let storage=transcript.textStorage,
              let data=try? storage.data(from:NSRange(location:0,length:storage.length),documentAttributes:[.documentType:NSAttributedString.DocumentType.rtf]) else { return }
        do {
            try FileManager.default.createDirectory(at:historyURL.deletingLastPathComponent(),withIntermediateDirectories:true)
            try data.write(to:historyURL,options:.atomic)
            try FileManager.default.setAttributes([.posixPermissions:0o600],ofItemAtPath:historyURL.path)
        } catch { phase.stringValue="聊天记录暂时无法保存，当前窗口仍可查看。" }
    }
    var followsHistoryTail: Bool {
        guard let scroll=transcript.enclosingScrollView else { return true }
        return scroll.documentVisibleRect.maxY >= transcript.bounds.maxY - 48
    }
    func append(_ who: String, _ text: String) {
        let follow=followsHistoryTail
        let header = NSAttributedString(string: "\(who)\n", attributes: [
            .font: NSFont.systemFont(ofSize: 12, weight: .semibold), .foregroundColor: NSColor.secondaryLabelColor])
        transcript.textStorage?.append(header)
        transcript.textStorage?.append(NSAttributedString(string: text + "\n\n", attributes: [
            .font: NSFont.systemFont(ofSize: 15), .foregroundColor: NSColor.labelColor]))
        if follow { transcript.scrollToEndOfDocument(nil) }
    }
    func launchWorker() {
        guard worker?.isRunning != true else { return }
        let executable = root.appendingPathComponent(".venv/bin/python")
        guard FileManager.default.isExecutableFile(atPath: executable.path) else {
            phase.stringValue = "找不到本地语音环境，请检查项目路径。"; showSubtitle(phase.stringValue); return
        }
        let process = Process(); let stdin = Pipe(); let stdout = Pipe()
        process.executableURL = executable
        process.arguments = [root.appendingPathComponent("scripts/chat_worker.py").path]
        process.currentDirectoryURL = root
        var env = ProcessInfo.processInfo.environment
        env["PATH"] = "\(FileManager.default.homeDirectoryForCurrentUser.path)/.npm-global/bin:/opt/homebrew/bin:/usr/bin:/bin:" + (env["PATH"] ?? "")
        env["PYTHONUNBUFFERED"] = "1"; process.environment = env
        process.standardInput = stdin; process.standardOutput = stdout
        let logURL = root.appendingPathComponent("runtime/voice-worker.log")
        if !FileManager.default.fileExists(atPath: logURL.path) { FileManager.default.createFile(atPath: logURL.path, contents: nil) }
        let log = try? FileHandle(forWritingTo: logURL); log?.seekToEndOfFile()
        process.standardError = log ?? FileHandle.nullDevice
        process.terminationHandler = { [weak self] _ in
            DispatchQueue.main.async {
                guard let self = self, !self.shuttingDown, self.worker === process else { return }
                self.generating = false; self.currentID = nil; self.streamSpeaking = false; self.reactionUntil = 0
                self.phase.stringValue = "语音连接已退出；重新发送可再连接。"
                self.requestedBoard=nil;self.lectureDelegatedTask=nil
                self.boardController?.captions.setInputEnabled(true)
                if self.lectureMode { self.boardController?.showError(self.phase.stringValue) }
            }
        }
        do {
            try process.run(); worker = process; workerInput = stdin.fileHandleForWriting
            workerOutput = stdout.fileHandleForReading
            outputQueue.async { [weak self] in
                while true {
                    let data = stdout.fileHandleForReading.availableData
                    guard !data.isEmpty else { break }
                    guard let self = self else { break }
                    self.receiveBuffer.append(data)
                    while let end = self.receiveBuffer.firstIndex(of: 10) {
                        let line = self.receiveBuffer[..<end]; self.receiveBuffer.removeSubrange(...end)
                        if let event = try? JSONSerialization.jsonObject(with: line) as? [String: Any] {
                            DispatchQueue.main.async { [weak self] in self?.receive(event) }
                        }
                    }
                }
            }
            sendCommand(["type":"configure", "settings":preferences.payload])
            sendCommand(["type":"notifications", "enabled":notificationEnabled])
        } catch { phase.stringValue = "无法启动语音服务：\(error.localizedDescription)" }
        try? log?.close()
    }
    func sendCommand(_ command: [String: Any]) {
        guard let data = try? JSONSerialization.data(withJSONObject: command), let handle = workerInput else { return }
        inputQueue.async { try? handle.write(contentsOf: data + Data([10])) }
    }
    @objc func sendText() {
        let text = input.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return }
        guard text.count <= (taskMode ? 16000:4000) else { phase.stringValue = "消息太长，请分开说。"; return }
        suggestions.hide();input.stringValue = ""; submit(["text":text])
    }
    func submit(_ payload: [String: Any]) {
        if recorder != nil { finishRecording(send: false) }
        cancelChat()
        lectureCurrentTurn=lectureMode && boardController?.window.isVisible == true
        if lectureCurrentTurn { boardController?.setBusy(true);boardController?.captions.setInputEnabled(false) }
        if let text=payload["text"] as? String { setDisplayedUser(text) }
        else { setDisplayedUser(lectureCurrentTurn ? "":"正在识别刚才的语音…") }
        launchWorker()
        guard worker?.isRunning == true else {
            if lectureCurrentTurn { boardController?.showError("连接尚未启动，请再试一次。");boardController?.captions.setInputEnabled(true) }
            return
        }
        let id = UUID().uuidString; currentID = id; currentUser = ""; currentAnswer = ""; activeSubtitle = ""; pendingReplyMessage=""; noticeMessage=""; generating = true
        showSubtitle("让我想一想…")
        var command = payload; command["type"] = "chat"; command["id"] = id; command["history"] = history; command["settings"] = preferences.payload
        command["mode"] = taskMode ? "task":"chat"
        if let project=selectedProject { command["project"]=project }
        command["attachments"]=attachmentPaths
        if let text=payload["text"] as? String {
            let selection=ComposerSubmission.parse(text,projects:projects,selected:selectedProject,taskMode:taskMode)
            command["text"]=selection.text;command["mode"]=selection.mode;command["project"]=selection.project
        }
        if lectureCurrentTurn {
            command["mode"]="lesson";command["project"]=nil;command["attachments"]=[]
            command["text"]=payload["text"];command["board_id"]=boardController?.board["task_id"]
            command["history"]=[]
        }
        if preferences.provider == "deepseek" && command["mode"] as? String != "task" && command["project"] == nil && attachmentPaths.isEmpty {
            do { command["api_key"] = try DeepSeekKeychain.read() ?? "" }
            catch {
                if let path = payload["audio"] as? String { try? FileManager.default.removeItem(atPath:path) }
                generating = false; showSubtitle(error.localizedDescription);if lectureCurrentTurn { boardController?.showError(error.localizedDescription);boardController?.captions.setInputEnabled(true) };return
            }
        }
        phase.stringValue = payload["audio"] != nil ? "正在识别语音…" : "正在连接 Codex…"
        sendCommand(command)
        attachmentPaths=[];updateComposerContext()
    }
    func receive(_ event: [String: Any]) {
        let type = event["type"] as? String ?? ""
        if receiveBoard(event) { return }
        if type == "ready" { sendCommand(["type":"projects"]);return }
        if type == "projects" { projectsLoaded=true;projects=event["projects"] as? [[String:String]] ?? [];updateComposerContext();updateSuggestions();return }
        if type == "tasks" { delegatedTasks=event["tasks"] as? [[String:Any]] ?? [];tasksController?.update(delegatedTasks);return }
        if type == "task_update",let task=event["task"] as? [String:Any],let ident=task["id"] as? String {
            delegatedTasks.removeAll { $0["id"] as? String == ident };delegatedTasks.append(task);tasksController?.update(delegatedTasks)
            let state=task["status"] as? String ?? ""
            if ["completed","failed","interrupted"].contains(state) && !notifiedTasks.contains(ident) {
                notifiedTasks.insert(ident)
                let result=task["result"] as? String ?? ""
                append("委托结果 · \(task["title"] as? String ?? "任务")",result);saveTranscript()
                if task["presentation"] as? String == "brief" {
                    if preferences.taskDelivery == "auto" { pendingBriefTasks.append(ident) }
                } else {
                    if preferences.taskDelivery == "auto" { autoAwaitingBoards.insert(ident) }
                    if state == "interrupted" { prepareBoard(ident) }
                }
            } else if state == "waiting" {
                noticeMessage="任务需要你处理：\(task["title"] as? String ?? "")。右键查看委托任务。";renderReplyText()
            } else if state == "queued" {
                notifiedTasks.remove(ident);boards.removeValue(forKey:ident);pendingBriefTasks.removeAll { $0 == ident }
                automaticBoards.removeAll { $0 == ident };autoAwaitingBoards.remove(ident)
                if boardSpeechTask == ident { stopBoardSpeech() }
            }
            if lectureMode && lectureDelegatedTask==ident {
                if ["completed","failed","interrupted"].contains(state) { prepareBoard(ident) }
                else if state=="waiting" { boardController?.showError("查询需要你的确认，请在委托任务中处理。") }
            }
            return
        }
        if type == "task_voice_error" { append("任务提示","结果已保存，语音提醒暂时失败。");saveTranscript();return }
        if type == "task_service_error" { noticeMessage=event["message"] as? String ?? "任务服务不可用";renderReplyText();return }
        if type == "hooks" {
            hookLabel.stringValue = (event["connected"] as? Bool == true)
                ? "任务播报 · 已收到 Codex 事件"
                : "任务播报 · 尚未收到事件，可在 Codex CLI 的 /hooks 中检查信任"
            return
        }
        if type == "notice" || type == "preview" {
            guard (notificationEnabled || type == "preview"), let encoded = event["data"] as? String, let data = Data(base64Encoded: encoded) else { return }
            append("任务提醒", event["subtitle"] as? String ?? "任务需要你查看。"); saveTranscript()
            clips.append((data, true, event["subtitle"] as? String ?? "这一轮处理结束了，来看看结果吧。")); playNext(); return
        }
        if type == "notice_error" {
            hookLabel.stringValue = event["message"] as? String ?? "提醒失败"
            append("提醒",hookLabel.stringValue);saveTranscript()
            noticeMessage=hookLabel.stringValue; renderReplyText()
            return
        }
        guard let id = event["id"] as? String, id == currentID else { return }
        switch type {
        case "phase": if player == nil && !streamPlayer.isBusy { phase.stringValue = event["value"] as? String ?? "" }
        case "lesson":
            guard let job=event["job"] as? [String:Any],let ident=job["id"] as? String else { return }
            generating=false;chatBoards[ident]=job;requestedBoard=ident
            currentAnswer="我把这个问题整理到小黑板上，分重点给你讲。"
            let lessonText=job["result"] as? String ?? ""
            history += [["role":"user","content":currentUser],["role":"assistant","content":String(lessonText.prefix(6000))]]
            history=Array(history.suffix(20))
            append("小黑板讲解",lessonText);saveTranscript();renderReplyText()
            ensureBoardWindow();enterLecture()
            boardController?.captions.setUserText(currentUser)
            if job["parent_id"] == nil {
                boardController?.show(["task_id":ident,"title":job["title"] as? String ?? "讲解","preparing":true,"takeaway":"","original":job["result"] as? String ?? ""])
            } else { boardController?.setBusy(true) }
            boardController?.captions.setInputEnabled(false)
        case "delegated":
            if lectureCurrentTurn,let task=event["task_id"] as? String {
                lectureDelegatedTask=task;requestedBoard=task
                boardController?.setBusy(true)
                if let done=delegatedTasks.first(where:{$0["id"] as? String==task}),["completed","failed","interrupted"].contains(done["status"] as? String ?? "") { prepareBoard(task) }
            }
            generating=false
            currentAnswer=event["text"] as? String ?? "已经交给 Codex 处理。"
            append("委托",currentAnswer);saveTranscript();renderReplyText()
            phase.stringValue="后台任务进行中，可右键切回普通聊天。"
        case "memory":
            let message = event["text"] as? String ?? ""
            append("记忆", message); saveTranscript()
            pendingReplyMessage = message; renderReplyText()
        case "model": connection.stringValue = "Codex · \(event["text"] as? String ?? "") · 订阅额度"
        case "user":
            currentUser = event["text"] as? String ?? ""; setDisplayedUser(currentUser); append("你", currentUser)
            append("洛琪希", "")
            replyInsertIndex = max(0, (transcript.textStorage?.length ?? 0) - 2); saveTranscript()
        case "delta":
            let delta = event["text"] as? String ?? ""; currentAnswer += delta
            // Keep insertion anchored even when a task notice arrives between deltas.
            let follow=followsHistoryTail
            let storage = transcript.textStorage!
            let insertion = min(replyInsertIndex, storage.length)
            storage.insert(NSAttributedString(string: delta, attributes: [.font:NSFont.systemFont(ofSize:15),
                .foregroundColor:NSColor.labelColor]), at: insertion)
            replyInsertIndex += (delta as NSString).length
            if follow { transcript.scrollToEndOfDocument(nil) }
            renderReplyText()
        case "audio_start":
            guard let stream = event["stream"] as? String else { return }
            streamPlayer.suspended = player != nil || recorder != nil
            streamPlayer.start(stream, subtitle: event["subtitle"] as? String ?? "", sampleRate: event["sample_rate"] as? Int ?? 0, expression: event["expression"] as? String ?? "calm")
        case "audio_chunk":
            if let stream = event["stream"] as? String, let encoded = event["data"] as? String,
               let data = Data(base64Encoded: encoded) { streamPlayer.append(stream, data: data) }
        case "audio_end":
            if let stream = event["stream"] as? String { streamPlayer.end(stream) }
        case "audio":
            if let encoded = event["data"] as? String, let data = Data(base64Encoded: encoded) {
                clips.append((data, false, event["subtitle"] as? String ?? currentAnswer)); playNext()
            }
        case "done":
            generating = false
            streamPlayer.finishOpenSegments()
            if !currentUser.isEmpty && !currentAnswer.isEmpty {
                history += [["role":"user", "content":currentUser], ["role":"assistant", "content":currentAnswer]]
                if history.count > 20 { history.removeFirst(history.count - 20) }
            }
            let warning = event["warning"] as? String ?? ""
            if !warning.isEmpty { pendingReplyMessage=warning }
            saveTranscript(); playNext(); showReplyWhenIdle()
            phase.stringValue = !warning.isEmpty ? warning : player == nil && !streamPlayer.isBusy ? "我在听，随时可以继续。" : "洛琪希正在说话…"
        case "error":
            generating = false; holdReaction("failed", seconds: 2); streamPlayer.finishOpenSegments(); phase.stringValue = event["message"] as? String ?? "连接失败，请重试。"
            pendingReplyMessage=phase.stringValue
            if lectureCurrentTurn { boardController?.showError(pendingReplyMessage);boardController?.captions.setInputEnabled(true) }
            append("提示",pendingReplyMessage);saveTranscript();showReplyWhenIdle()
        default: break
        }
    }
    func playNext() {
        guard player == nil, !streamPlayer.isBusy, recorder == nil else { return }
        if boardSpeechRequest != nil && !boardAudioDone && boardAudioQueue.isEmpty && !generating { boardController?.setBusy(true);return }
        if !boardAudioQueue.isEmpty, !generating {
            let audio=boardAudioQueue.removeFirst();activeBoardSpeech=true;currentLessonAudio=audio
            clips.insert((audio.data,true,audio.caption),at:0)
        }
        guard !clips.isEmpty else { return }
        // Task notices wait through synthesis gaps, so they cannot replace an unfinished reply.
        if generating && clips.first?.1 == true { return }
        let clip = clips.removeFirst()
        do {
            let audio = try AVAudioPlayer(data: clip.0); audio.delegate = self; player = audio
            activeSubtitle = clip.2; activeIsNotice = clip.1
            if !activeBoardSpeech && clip.1 { noticeMessage=clip.2; renderReplyText() }
            else if !activeBoardSpeech && currentAnswer.isEmpty { showSubtitle(clip.2) }
            guard audio.play() else { throw NSError(domain: "播放失败", code: 1) }
            if activeBoardSpeech,let segment=currentLessonAudio { boardController?.beginSegment(segment) }
            phase.stringValue = clip.1 ? "任务语音提醒…" : "洛琪希正在说话…"
        } catch {
            player = nil
            if activeBoardSpeech { activeBoardSpeech=false;boardSpeechRequest=nil;boardSpeechTask=nil;boardController?.showError("讲解播放失败，可以重试。") }
            phase.stringValue = "音频播放失败，请检查声音输出。"; playNext()
        }
    }
    func audioPlayerDidFinishPlaying(_ player: AVAudioPlayer, successfully flag: Bool) {
        self.player = nil; activeIsNotice = false; streamPlayer.suspended = false
        if activeBoardSpeech {
            activeBoardSpeech=false;currentLessonAudio=nil
            finishBoardIfIdle()
        }
        if clips.isEmpty { phase.stringValue = generating ? "洛琪希还在组织下一句话…" : "我在听，随时可以继续。" }
        playNext(); showReplyWhenIdle()
    }
    func cancelChat() {
        suggestions.hide()
        if lectureMode || requestedBoard?.hasPrefix("chat-") == true { requestedBoard=nil }
        lectureDelegatedTask=nil
        boardController?.captions.setInputEnabled(true)
        stopBoardSpeech()
        saveTranscript()
        currentID = nil; generating = false; streamSpeaking = false; reactionUntil = 0; reactionPose = nil; replyExpression = "calm"
        sendCommand(["type":"cancel"])
        player?.stop(); player = nil; streamPlayer.stop(); clips.removeAll(); activeIsNotice = false
    }
    @objc func stopAll() {
        finishRecording(send: false); cancelChat(); phase.stringValue = "已停止。"
    }
    @objc func clearChat() {
        stopAll(); history.removeAll()
        currentUser="";currentAnswer="";pendingReplyMessage="";noticeMessage="";setDisplayedUser("")
        append("新对话", "我们开始新的聊天吧。");saveTranscript()
        showSubtitle("我们开始新的聊天吧。")
        phase.stringValue = "新对话已开始。"
    }
    func beginRecording() {
        guard recorder == nil else { return }
        switch AVCaptureDevice.authorizationStatus(for: .audio) {
        case .authorized: break
        case .notDetermined:
            showInputNotice("第一次使用，请允许麦克风访问，然后再次按住说话。")
            AVCaptureDevice.requestAccess(for: .audio) { [weak self] granted in
                DispatchQueue.main.async { self?.showInputNotice(granted ? "麦克风已就绪，再次按住说话即可。" : "麦克风未授权；仍可打字聊天。") }
            }
            return
        default:
            showInputNotice("请在系统设置 → 隐私与安全性 → 麦克风中允许洛琪希，也可以先打字聊天。"); return
        }
        cancelChat()
        do {
            let directory = root.appendingPathComponent("runtime/recordings")
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            let url = directory.appendingPathComponent(UUID().uuidString + ".wav")
            let audio = try AVAudioRecorder(url: url, settings: [AVFormatIDKey:kAudioFormatLinearPCM,
                AVSampleRateKey:16000, AVNumberOfChannelsKey:1, AVLinearPCMBitDepthKey:16,
                AVLinearPCMIsFloatKey:false, AVLinearPCMIsBigEndianKey:false])
            guard audio.record() else { throw NSError(domain: "麦克风无法开始录音", code: 1) }
            recorder = audio; recordingURL = url; recordingBegan = Date()
            boardController?.captions.mic.contentTintColor = .systemRed
            talk.contentTintColor = .systemRed
            talk.image = NSImage(systemSymbolName:"mic.fill",accessibilityDescription:"正在录音，松开发送")
            phase.stringValue = "正在录音 · 松开发送 · 最长 60 秒"
            let timer = Timer(timeInterval: 0.2, repeats: true) { [weak self] _ in
                guard let self = self else { return }
                if Date().timeIntervalSince(self.recordingBegan) >= 60 { self.finishRecording(send: true) }
            }
            recordingTimer = timer; RunLoop.main.add(timer, forMode: .common)
        } catch { showInputNotice("录音失败：\(error.localizedDescription)") }
    }
    func finishRecording(send: Bool) {
        guard recorder != nil else { return }
        recorder?.stop(); recorder = nil; recordingTimer?.invalidate(); recordingTimer = nil
        let url = recordingURL; recordingURL = nil
        boardController?.captions.mic.contentTintColor = nil
        talk.contentTintColor = nil
        talk.image = NSImage(systemSymbolName:"mic",accessibilityDescription:"按住说话")
        if send, let url = url { submit(["audio":url.path]) }
        else if let url = url { try? FileManager.default.removeItem(at: url) }
    }
    @objc func toggleRecording() { if recorder == nil { beginRecording() } else { finishRecording(send: true) } }
    @objc func toggleNotifications() {
        notificationEnabled.toggle(); notifyToggle.state = notificationEnabled ? .on : .off
        sendCommand(["type":"notifications", "enabled":notificationEnabled]); onMenuChange?()
        if !notificationEnabled { clips.removeAll { $0.1 } }
    }
    @objc func previewNotice() {
        guard notificationEnabled else { hookLabel.stringValue = "请先开启任务语音播报，再试听。"; return }
        launchWorker(); hookLabel.stringValue = "正在准备提醒声音…"; sendCommand(["type":"preview"])
    }
    func windowWillClose(_ notification: Notification) { hideChat() }
    func shutdown() {
        shuttingDown = true; finishRecording(send: false); cancelChat()
        let handle = workerInput; workerInput = nil
        inputQueue.async { try? handle?.close() }
        // EOF lets the worker stop only the app-server/TTS processes it owns.
    }
    func renderQA(_ directory: String) {
        try? FileManager.default.createDirectory(atPath: directory, withIntermediateDirectories: true)
        for mode in [NSAppearance.Name.aqua, .darkAqua] {
            for (name, panel) in [("input", window!), ("subtitle", bubble as NSWindow)] {
                panel.appearance = NSAppearance(named: mode); panel.contentView?.layoutSubtreeIfNeeded()
                guard let view = panel.contentView, let bitmap = view.bitmapImageRepForCachingDisplay(in: view.bounds) else { continue }
                panel.appearance?.performAsCurrentDrawingAppearance { view.displayIfNeeded(); view.cacheDisplay(in: view.bounds, to: bitmap) }
                try? bitmap.representation(using: .png, properties: [:])?.write(to:
                    URL(fileURLWithPath: directory).appendingPathComponent("\(name)-\(mode.rawValue).png"))
            }
        }
    }
}
