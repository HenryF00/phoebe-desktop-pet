import Cocoa

struct CompanionNotes: Codable {
    var enabled = true
    var notes = ""
    static func read(_ url: URL) throws -> CompanionNotes {
        guard FileManager.default.fileExists(atPath: url.path) else { return CompanionNotes() }
        let value = try JSONDecoder().decode(CompanionNotes.self, from: Data(contentsOf: url))
        guard value.notes.count <= 4000 else { throw CocoaError(.fileReadCorruptFile) }
        return value
    }
    func save(_ url: URL) throws {
        guard notes.count <= 4000 else { throw CocoaError(.fileWriteOutOfSpace) }
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        let data = try JSONEncoder().encode(self)
        // .atomic alone can leave a briefly world-readable temporary file. Create it privately first.
        let temporary = url.deletingLastPathComponent().appendingPathComponent(".memory-" + UUID().uuidString)
        guard FileManager.default.createFile(atPath: temporary.path, contents: data,
                    attributes: [.posixPermissions: 0o600]) else { throw CocoaError(.fileWriteUnknown) }
        defer { try? FileManager.default.removeItem(at: temporary) }
        guard rename(temporary.path, url.path) == 0 else { throw CocoaError(.fileWriteUnknown) }
    }
}

final class CompanionMemoryController: NSObject {
    let url: URL
    let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 520, height: 420),
                          styleMask: [.titled, .closable, .resizable], backing: .buffered, defer: false)
    let enabled = NSButton(checkboxWithTitle: "在聊天中使用长期记忆", target: nil, action: nil)
    let editor = NSTextView()
    let status = NSTextField(wrappingLabelWithString: "")
    init(url: URL) {
        self.url = url
        super.init()
        window.contentView = SettingsBackground(frame: NSRect(x: 0, y: 0, width: 520, height: 420))
        window.title = "洛琪希 · 记住的事"; window.isReleasedWhenClosed = false
        window.minSize = NSSize(width: 460, height: 340); window.center()
        let hint = NSTextField(wrappingLabelWithString: "每行一件希望她记住的事，例如称呼和聊天偏好。也可在聊天中说“请记住：……”或“忘记：完全匹配的内容”。内容保存在本机，启用时会随聊天发送给所选模型。")
        hint.font = .systemFont(ofSize: 12); hint.textColor = .secondaryLabelColor
        let title = NSTextField(labelWithString: "长期偏好与约定（最多 4000 字）")
        editor.isRichText = false; editor.font = .systemFont(ofSize: 14)
        editor.textContainerInset = NSSize(width: 10, height: 10)
        editor.setAccessibilityLabel("长期偏好与约定，每行一条")
        let scroll = NSScrollView(); scroll.hasVerticalScroller = true; scroll.borderType = .bezelBorder
        scroll.documentView = editor; editor.isVerticallyResizable = true
        editor.autoresizingMask = [.width]; editor.textContainer?.widthTracksTextView = true
        let save = NSButton(title: "保存", target: self, action: #selector(saveNotes)); save.bezelStyle = .rounded
        save.keyEquivalent = "\r"
        let clear = NSButton(title: "清空内容", target: self, action: #selector(clearDraft)); clear.bezelStyle = .rounded
        let buttons = NSStackView(views: [clear, NSView(), save]); buttons.spacing = 8
        status.font = .systemFont(ofSize: 12)
        let stack = NSStackView(views: [enabled, hint, title, scroll, status, buttons])
        stack.orientation = .vertical; stack.alignment = .leading; stack.spacing = 12
        stack.translatesAutoresizingMaskIntoConstraints = false
        window.contentView!.addSubview(stack)
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: window.contentView!.leadingAnchor, constant: 20),
            stack.trailingAnchor.constraint(equalTo: window.contentView!.trailingAnchor, constant: -20),
            stack.topAnchor.constraint(equalTo: window.contentView!.topAnchor, constant: 20),
            stack.bottomAnchor.constraint(equalTo: window.contentView!.bottomAnchor, constant: -20),
            scroll.heightAnchor.constraint(greaterThanOrEqualToConstant: 140)])
        for view in stack.arrangedSubviews { view.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true }
    }
    func reload() {
        do {
            let value = try CompanionNotes.read(url)
            enabled.state = value.enabled ? .on : .off; editor.string = value.notes; status.stringValue = ""
        } catch { status.stringValue = "记忆读取失败；检查内容并保存可重建。" }
    }
    func show() {
        reload();window.makeKeyAndOrderFront(nil); NSApp.activate(ignoringOtherApps: true)
    }
    @objc func clearDraft() { editor.string = ""; status.stringValue = "内容已清空，点击保存后生效。" }
    @objc func saveNotes() {
        guard editor.string.count <= 4000 else { status.stringValue = "内容超过 4000 字，请精简后保存。"; return }
        do {
            try CompanionNotes(enabled: enabled.state == .on, notes: editor.string).save(url)
            status.stringValue = "已保存，下次对话生效。新对话不会清除这些记忆。"
        } catch { status.stringValue = "未能保存，请检查文件权限后重试。" }
    }
}
