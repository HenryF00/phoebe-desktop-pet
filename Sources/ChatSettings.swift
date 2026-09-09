import Cocoa
import Security

final class SettingsBackground: NSView {
    override func draw(_ dirtyRect:NSRect) { NSColor.windowBackgroundColor.setFill(); dirtyRect.fill() }
}

struct ChatPreferences: Equatable {
    var provider = "codex"
    var taskDelivery = "auto"
    var boardSpeech = true
    var subtitleLanguage = "zh"
    var voiceLanguage = "ja"
    var codexModel = ""
    var deepseekModel = "deepseek-v4-flash"
    static func load() -> ChatPreferences {
        let d = UserDefaults.standard.dictionary(forKey: "chatPreferences") ?? [:]
        var p = ChatPreferences()
        if let v=d["provider"] as? String, ["codex","deepseek"].contains(v) { p.provider=v }
        if let v=d["subtitle_language"] as? String, ["zh","en","ja"].contains(v) { p.subtitleLanguage=v }
        if let v=d["voice_language"] as? String, ["zh","en","ja"].contains(v) { p.voiceLanguage=v }
        p.taskDelivery = d["task_delivery"] as? String == "manual" ? "manual":"auto"
        p.boardSpeech = d["board_speech"] as? String != "false"
        p.codexModel = d["codex_model"] as? String ?? ""
        p.deepseekModel = d["deepseek_model"] as? String ?? "deepseek-v4-flash"
        return p
    }
    var payload: [String: String] {
        ["provider":provider, "subtitle_language":subtitleLanguage, "voice_language":voiceLanguage,
         "codex_model":codexModel, "deepseek_model":deepseekModel, "task_delivery":taskDelivery,"board_speech":boardSpeech ? "true":"false"]
    }
    func save() { UserDefaults.standard.set(payload, forKey: "chatPreferences") }
}

enum DeepSeekKeychain {
    static let query: [String:Any] = [kSecClass as String:kSecClassGenericPassword,
        kSecAttrService as String:"local.roxy.hd.pet.deepseek", kSecAttrAccount as String:"api-key"]
    static func read() throws -> String? {
        var q = query; q[kSecReturnData as String] = true; q[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: CFTypeRef?
        let status = SecItemCopyMatching(q as CFDictionary, &result)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess, let data = result as? Data else { throw failure(status) }
        return String(data:data,encoding:.utf8)
    }
    static func save(_ secret: String) throws {
        let data = Data(secret.utf8)
        var status = SecItemUpdate(query as CFDictionary, [kSecValueData as String:data] as CFDictionary)
        if status == errSecItemNotFound {
            var q = query; q[kSecValueData as String]=data
            q[kSecAttrAccessible as String]=kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
            status=SecItemAdd(q as CFDictionary,nil)
        }
        guard status == errSecSuccess else { throw failure(status) }
    }
    static func clear() throws {
        let status=SecItemDelete(query as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else { throw failure(status) }
    }
    static func failure(_ code: OSStatus) -> NSError {
        NSError(domain:"RoxyKeychain",code:Int(code),userInfo:[NSLocalizedDescriptionKey:"无法访问 macOS 钥匙串，请检查系统弹窗后重试。"])
    }
}

final class ChatSettingsController: NSObject, NSWindowDelegate {
    let window: NSWindow
    let caption = NSPopUpButton(frame:.zero,pullsDown:false)
    let speech = NSPopUpButton(frame:.zero,pullsDown:false)
    let provider = NSPopUpButton(frame:.zero,pullsDown:false)
    let codexModel = NSTextField(string:"")
    let deepseekModel = NSComboBox()
    let key = NSSecureTextField(string:"")
    let status = NSTextField(wrappingLabelWithString:"")
    let detail = NSTextField(wrappingLabelWithString:"")
    var codexRow: NSView!
    var deepseekRows: NSStackView!
    var onSave: ((ChatPreferences) -> Void)?
    var onPreview: (() -> Void)?
    var onMemory: (() -> Void)?
    var onReset: (() -> Void)?
    var persist: (ChatPreferences) -> Void = { $0.save() }
    var saveKey: (String) throws -> Void = DeepSeekKeychain.save
    override init() {
        window=NSWindow(contentRect:NSRect(x:0,y:0,width:520,height:500),
            styleMask:[.titled,.closable],backing:.buffered,defer:false)
        super.init()
        window.contentView=SettingsBackground(frame:NSRect(x:0,y:0,width:520,height:500))
        window.title="洛琪希 · 设置";window.isReleasedWhenClosed=false;window.delegate=self;window.center()
        caption.addItems(withTitles:["中文","English","日本語"])
        speech.addItems(withTitles:["日语","中文","English"])
        provider.addItems(withTitles:["Codex · 订阅账号","DeepSeek · API Key"])
        provider.target=self;provider.action=#selector(providerChanged)
        codexModel.placeholderString="留空使用 Codex 默认模型"
        deepseekModel.addItems(withObjectValues:["deepseek-v4-flash","deepseek-v4-pro"])
        key.placeholderString="输入新密钥；留空保留已保存的密钥"
        for (control,name) in [(caption as NSControl,"字幕语言"),(speech,"语音语言"),(provider,"聊天模型来源"),
                               (codexModel,"Codex 模型名称"),(deepseekModel,"DeepSeek 模型名称"),(key,"DeepSeek API Key")] {
            control.setAccessibilityLabel(name)
        }
        codexRow=row("模型",codexModel)
        let clear=NSButton(title:"清除已存密钥",target:self,action:#selector(clearKey));clear.bezelStyle = .rounded
        deepseekRows=NSStackView(views:[row("模型",deepseekModel),row("API Key",key),row("",clear)])
        deepseekRows.orientation = .vertical;deepseekRows.alignment = .leading;deepseekRows.spacing=10
        let hint=NSTextField(wrappingLabelWithString:"字幕和声音可以使用不同语言。沿用同一参考声音。中英文逐句播放；英文可能有明显口音，音色相似度也会变化。")
        hint.font = .systemFont(ofSize:12);hint.textColor = .secondaryLabelColor
        detail.font = .systemFont(ofSize:12);detail.textColor = .secondaryLabelColor
        status.font = .systemFont(ofSize:12)
        let save=NSButton(title:"保存",target:self,action:#selector(saveSettings));save.bezelStyle = .rounded;save.keyEquivalent="\r"
        let preview=NSButton(title:"保存并试听声音",target:self,action:#selector(previewVoice));preview.bezelStyle = .rounded
        let reset=NSButton(title:"新对话",target:self,action:#selector(resetChat));reset.bezelStyle = .rounded
        let memory=NSButton(title:"记住的事…",target:self,action:#selector(showMemory));memory.bezelStyle = .rounded
        let buttons=NSStackView(views:[reset,memory,NSView(),preview,save]);buttons.orientation = .horizontal;buttons.spacing=8
        let title=NSTextField(labelWithString:"让洛琪希按你的习惯说话")
        title.font = .systemFont(ofSize:21,weight:.semibold)
        let stack=NSStackView(views:[title,row("字幕语言",caption),row("语音语言",speech),hint,
            row("模型来源",provider),codexRow,deepseekRows,detail,status,NSView(),buttons])
        stack.orientation = .vertical;stack.alignment = .leading;stack.spacing=12;stack.translatesAutoresizingMaskIntoConstraints=false
        window.contentView!.addSubview(stack)
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo:window.contentView!.leadingAnchor,constant:24),
            stack.trailingAnchor.constraint(equalTo:window.contentView!.trailingAnchor,constant:-24),
            stack.topAnchor.constraint(equalTo:window.contentView!.topAnchor,constant:24),
            stack.bottomAnchor.constraint(equalTo:window.contentView!.bottomAnchor,constant:-20)])
        for view in stack.arrangedSubviews { view.widthAnchor.constraint(equalTo:stack.widthAnchor).isActive=true }
        for view in deepseekRows.arrangedSubviews { view.widthAnchor.constraint(equalTo:deepseekRows.widthAnchor).isActive=true }
        load(ChatPreferences.load())
    }
    func row(_ title:String,_ control:NSView) -> NSView {
        let label=NSTextField(labelWithString:title);label.font = .systemFont(ofSize:13)
        label.widthAnchor.constraint(equalToConstant:82).isActive=true
        let row=NSStackView(views:[label,control]);row.orientation = .horizontal;row.spacing=12
        control.heightAnchor.constraint(greaterThanOrEqualToConstant:26).isActive=true
        return row
    }
    func load(_ prefs:ChatPreferences) {
        caption.selectItem(at:["zh","en","ja"].firstIndex(of:prefs.subtitleLanguage) ?? 0)
        speech.selectItem(at:["ja","zh","en"].firstIndex(of:prefs.voiceLanguage) ?? 0)
        provider.selectItem(at:prefs.provider == "deepseek" ? 1:0)
        codexModel.stringValue=prefs.codexModel;deepseekModel.stringValue=prefs.deepseekModel
        key.stringValue="";status.stringValue="";providerChanged()
    }
    func show() { load(ChatPreferences.load());window.makeKeyAndOrderFront(nil);NSApp.activate(ignoringOtherApps:true) }
    @objc func providerChanged() {
        let isDeepSeek=provider.indexOfSelectedItem == 1
        codexRow.isHidden=isDeepSeek;deepseekRows.isHidden = !isDeepSeek
        detail.stringValue = isDeepSeek ? "连接 api.deepseek.com，按 DeepSeek API 计费。密钥仅保存在本机钥匙串。" : "使用 Codex 已登录的 ChatGPT 账号及共享额度，无需填写 API Key。"
        window.contentView?.layoutSubtreeIfNeeded()
    }
    @discardableResult @objc func saveSettings() -> Bool {
        var prefs=ChatPreferences.load()
        prefs.subtitleLanguage=["zh","en","ja"][max(0,caption.indexOfSelectedItem)]
        prefs.voiceLanguage=["ja","zh","en"][max(0,speech.indexOfSelectedItem)]
        prefs.provider=provider.indexOfSelectedItem == 1 ? "deepseek":"codex"
        prefs.codexModel=codexModel.stringValue.trimmingCharacters(in:.whitespacesAndNewlines)
        prefs.deepseekModel=deepseekModel.stringValue.trimmingCharacters(in:.whitespacesAndNewlines)
        if prefs.deepseekModel.isEmpty { prefs.deepseekModel="deepseek-v4-flash" }
        guard prefs.codexModel.count<=120,prefs.deepseekModel.count<=120,
              !prefs.codexModel.contains("\n"),!prefs.deepseekModel.contains("\n") else { status.stringValue="模型名称不正确，请检查。";return false }
        do {
            let secret=key.stringValue.trimmingCharacters(in:.whitespacesAndNewlines)
            if !secret.isEmpty { try saveKey(secret);key.stringValue="" }
            persist(prefs);onSave?(prefs);status.stringValue="已保存，下次对话使用新设置。";return true
        } catch { status.stringValue=error.localizedDescription;return false }
    }
    @objc func previewVoice() { if saveSettings() { onPreview?();status.stringValue="已保存，正在准备试听声音…" } }
    @objc func clearKey() {
        do { try DeepSeekKeychain.clear();key.stringValue="";status.stringValue="已清除 DeepSeek 密钥。" }
        catch { status.stringValue=error.localizedDescription }
    }
    @objc func showMemory() { onMemory?() }
    @objc func resetChat() { onReset?();status.stringValue="已开始新对话，历史记录保留。" }
    func windowWillClose(_ notification:Notification) { key.stringValue="" }
}
