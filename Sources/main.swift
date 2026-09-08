import Cocoa

struct Clip: Decodable {
    let label: String
    let frames: [String]
    let durations: [Double]
}
struct AnimationManifest: Decodable {
    let width: Int
    let height: Int
    let bodyHeight: Double
    let clips: [String: Clip]
}
final class PetPanel: NSPanel { override var canBecomeKey: Bool { true } }

// Keep the entire transparent pet canvas within the nearest usable display.
func visiblePetFrame(_ proposed: NSRect, pointer: NSPoint? = nil) -> NSRect {
    let screen = pointer.flatMap { point in NSScreen.screens.first { $0.frame.contains(point) } }
        ?? NSScreen.screens.max { a, b in
            let ar = a.visibleFrame.intersection(proposed), br = b.visibleFrame.intersection(proposed)
            return (ar.isNull ? 0 : ar.width * ar.height) < (br.isNull ? 0 : br.width * br.height)
        }
    guard let area = screen?.visibleFrame else { return proposed }
    var result = proposed
    result.origin.x = min(max(proposed.minX, area.minX), max(area.minX, area.maxX - proposed.width))
    result.origin.y = min(max(proposed.minY, area.minY), max(area.minY, area.maxY - proposed.height))
    return result
}


final class PetView: NSView {
    var image: NSImage?
    var menuProvider: (() -> NSMenu)?
    var action: ((String) -> Void)?
    var dragStart: NSPoint?
    var windowStart: NSPoint?
    var dragged = false
    var jumpOffset: CGFloat = 0
    override var mouseDownCanMoveWindow: Bool { false }
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
    override func draw(_ dirtyRect: NSRect) {
        NSColor.clear.setFill(); bounds.fill(using: .copy)
        NSGraphicsContext.current?.imageInterpolation = .high
        let rect = bounds.offsetBy(dx: 0, dy: jumpOffset)
        image?.draw(in: rect, from: .zero, operation: .sourceOver, fraction: 1,
                    respectFlipped: false, hints: [.interpolation: NSImageInterpolation.high])
    }
    func isOpaqueAt(_ point: NSPoint) -> Bool {
        guard bounds.contains(point), let bitmap = image?.representations.compactMap({ $0 as? NSBitmapImageRep }).first else { return false }
        let x = min(bitmap.pixelsWide - 1, max(0, Int(point.x / bounds.width * CGFloat(bitmap.pixelsWide))))
        let y = min(bitmap.pixelsHigh - 1, max(0, Int((1 - (point.y - jumpOffset) / bounds.height) * CGFloat(bitmap.pixelsHigh))))
        return (bitmap.colorAt(x: x, y: y)?.alphaComponent ?? 0) > 0.05
    }
    override func mouseDown(with event: NSEvent) {
        if event.modifierFlags.contains(.control) { rightMouseDown(with: event); return }
        dragStart = NSEvent.mouseLocation; windowStart = window?.frame.origin; dragged = false
        if event.clickCount == 2 { action?("jumping") }
    }
    override func mouseDragged(with event: NSEvent) {
        guard let start = dragStart, let origin = windowStart else { return }
        let point = NSEvent.mouseLocation
        if abs(point.x - start.x) + abs(point.y - start.y) > 4 {
            dragged = true
            if let window = window {
                let proposed = NSRect(origin: NSPoint(x: origin.x + point.x - start.x, y: origin.y + point.y - start.y), size: window.frame.size)
                window.setFrameOrigin(visiblePetFrame(proposed, pointer: point).origin)
            }
            action?(event.deltaX < 0 ? "running-left" : "running-right")
        }
    }
    override func mouseUp(with event: NSEvent) {
        if dragged { action?("idle") }
        else if event.clickCount == 1 { action?("waving") }
        dragStart = nil; windowStart = nil
    }
    override func rightMouseDown(with event: NSEvent) {
        if let menu = menuProvider?() { NSMenu.popUpContextMenu(menu, with: event, for: self) }
    }
}

final class PetController: NSObject, NSApplicationDelegate, NSMenuDelegate {
    var manifest: AnimationManifest!
    var images: [String: [NSImage]] = [:]
    var panel: PetPanel!
    var pet: PetView!
    var statusItem: NSStatusItem!
    var timer: Timer?
    var state = "idle"
    var frameIndex = 0
    var frameElapsed = 0.0
    var actionElapsed = 0.0
    var previousTime = ProcessInfo.processInfo.systemUptime
    var height: CGFloat = 560
    var paused = false
    var menuOpen = false
    var automatic = true
    var nextGesture = 16.0
    var gazeUntil = 0.0
    var lastMouse = NSEvent.mouseLocation
    var lastDraw = ""
    var gazeCandidate = ""
    var gazeCandidateSince = 0.0
    var lastStateChange = 0.0
    var followCodex = true
    var codexState: String?
    var codexLabel = "尚未收到 Codex 事件"
    var lastCodexPoll = 0.0
    var manualUntil = 0.0
    let stateOrder = ["idle", "waving", "jumping", "running-right", "running-left", "running", "waiting", "review", "failed"]
    let settings = UserDefaults.standard

    func applicationDidFinishLaunching(_ notification: Notification) {
        if NSRunningApplication.runningApplications(withBundleIdentifier: "local.roxy.hd.pet").count > 1 {
            NSApp.terminate(nil); return
        }
        do {
            guard let root = Bundle.main.resourceURL else { throw NSError(domain: "RoxyHD", code: 1) }
            manifest = try JSONDecoder().decode(AnimationManifest.self, from: Data(contentsOf: root.appendingPathComponent("animations.json")))
            var cache: [String: NSImage] = [:]
            for (key, clip) in manifest.clips {
                guard !clip.frames.isEmpty, clip.frames.count == clip.durations.count,
                      clip.durations.allSatisfy({ $0 > 0 }) else { throw NSError(domain: "Invalid clip \(key)", code: 2) }
                images[key] = try clip.frames.map { path in
                    if let cached = cache[path] { return cached }
                    guard let data = try? Data(contentsOf: root.appendingPathComponent(path)),
                          let bitmap = NSBitmapImageRep(data: data), bitmap.bitmapData != nil else {
                        throw NSError(domain: "Missing frame \(path)", code: 3)
                    }
                    let image = NSImage(size: NSSize(width: manifest.width, height: manifest.height))
                    image.addRepresentation(bitmap); cache[path] = image
                    return image
                }
            }
            guard stateOrder.allSatisfy({ images[$0] != nil }) else { throw NSError(domain: "Missing animation", code: 4) }
        } catch {
            let alert = NSAlert(); alert.messageText = "无法载入洛琪希动画"
            alert.informativeText = error.localizedDescription; alert.runModal(); NSApp.terminate(nil); return
        }
        let saved = settings.double(forKey: "displayHeight")
        if (300...560).contains(saved) { height = saved }
        pet = PetView(frame: .zero)
        pet.setAccessibilityElement(true)
        pet.setAccessibilityRole(.image)
        pet.menuProvider = { [weak self] in self?.makeMenu() ?? NSMenu() }
        pet.action = { [weak self] state in
            self?.manualUntil = ProcessInfo.processInfo.systemUptime + 4
            self?.select(state)
        }
        panel = PetPanel(contentRect: NSRect(origin: .zero, size: windowSize()),
                         styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        panel.title = "洛琪希高清桌宠"
        panel.isOpaque = false; panel.backgroundColor = .clear; panel.hasShadow = false
        panel.level = .floating; panel.hidesOnDeactivate = false
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        panel.contentView = pet; pet.autoresizingMask = [.width, .height]
        resetPosition()
        if let savedPosition = settings.string(forKey: "position") {
            let origin = NSPointFromString(savedPosition)
            let rect = NSRect(origin: origin, size: panel.frame.size)
            if NSScreen.screens.contains(where: { $0.visibleFrame.intersection(rect).width > 100 && $0.visibleFrame.intersection(rect).height > 100 }) {
                panel.setFrameOrigin(visiblePetFrame(rect).origin)
            }
        }
        panel.orderFrontRegardless()
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        statusItem.button?.title = "Roxy HD"; refreshMenu(); redraw()
        let ticker = Timer(timeInterval: 1.0 / 30.0, repeats: true) { [weak self] _ in self?.tick() }
        RunLoop.main.add(ticker, forMode: .common); timer = ticker
        if let i = CommandLine.arguments.firstIndex(of: "--render-qa"), CommandLine.arguments.count > i + 1 {
            renderQA(directory: CommandLine.arguments[i + 1]); NSApp.terminate(nil)
        }
    }
    func windowSize() -> NSSize {
        let scale = height / manifest.bodyHeight
        return NSSize(width: CGFloat(manifest.width) * scale, height: CGFloat(manifest.height) * scale)
    }
    func select(_ newState: String) {
        guard images[newState] != nil else { return }
        if state != newState {
            lastStateChange = ProcessInfo.processInfo.systemUptime
            state = newState; frameIndex = 0; frameElapsed = 0; actionElapsed = 0
            redraw(); refreshMenu()
        }
    }
    func tick() {
        let now = ProcessInfo.processInfo.systemUptime
        let delta = min(now - previousTime, 0.12); previousTime = now
        if now - lastCodexPoll > 1 { lastCodexPoll = now; pollCodex() }
        if pet.dragStart == nil {
            let mouse = NSEvent.mouseLocation
            let local = NSPoint(x: mouse.x - panel.frame.minX, y: mouse.y - panel.frame.minY)
            panel.ignoresMouseEvents = !pet.isOpaqueAt(local)
        }
        guard !paused && !menuOpen else { return }
        let taskOwnsPose = followCodex && codexState != nil && now > manualUntil && pet.dragStart == nil
        if taskOwnsPose, let target = codexState, target != state { select(target) }
        actionElapsed += delta
        frameElapsed += delta
        if let clip = manifest.clips[state] {
            while frameElapsed >= clip.durations[frameIndex] {
                frameElapsed -= clip.durations[frameIndex]
                frameIndex = (frameIndex + 1) % clip.frames.count
            }
        }
        if !taskOwnsPose && state != "idle" && !state.hasPrefix("look-") && pet.dragStart == nil && actionElapsed > (state == "jumping" ? 0.85 : state == "waving" ? 2.0 : 3.8) {
            select("idle")
        }
        if automatic && !taskOwnsPose && pet.dragStart == nil {
            let mouse = NSEvent.mouseLocation
            let dx = mouse.x - panel.frame.midX, dy = mouse.y - panel.frame.midY
            let moved = hypot(mouse.x - lastMouse.x, mouse.y - lastMouse.y) > 3
            if hypot(dx, dy) < 620 && (moved || !gazeCandidate.isEmpty) && (state == "idle" || state.hasPrefix("look-")) {
                let angle = (atan2(dx, dy) * 180 / .pi + 360).truncatingRemainder(dividingBy: 360)
                let direction = (Int((angle / 90).rounded()) % 4) * 4
                let key = "look-\(direction)"
                var stableKey = key
                if state.hasPrefix("look-"), let current = Int(state.dropFirst(5)) {
                    let separation = abs((angle - Double(current * 90 / 4) + 540).truncatingRemainder(dividingBy: 360) - 180)
                    if separation < 58 { stableKey = state }
                }
                if gazeCandidate != stableKey { gazeCandidate = stableKey; gazeCandidateSince = now }
                if manifest.clips[stableKey] != nil && now - gazeCandidateSince >= 0.18 && now - lastStateChange >= 0.24 {
                    let changed = state != stableKey
                    select(stableKey)
                    if moved || changed { gazeUntil = now + 1.4 }
                }
            }
            lastMouse = mouse
            if state.hasPrefix("look-") && now > gazeUntil { gazeCandidate = ""; select("idle") }
            nextGesture -= delta
            if nextGesture <= 0 && state == "idle" && !followCodex {
                select(["waving", "review", "waiting"].randomElement()!)
                nextGesture = Double.random(in: 18...30)
            }
        }
        let lift: CGFloat = state == "jumping" ? CGFloat(sin(min(1, actionElapsed / 0.85) * .pi)) * height * 0.025 : 0
        if abs(pet.jumpOffset - lift) > 0.1 { pet.jumpOffset = lift; pet.needsDisplay = true }
        redraw()
    }
    func redraw() {
        let key = "\(state):\(frameIndex)"
        guard key != lastDraw, let frames = images[state], frameIndex < frames.count else { return }
        lastDraw = key
        pet.image = frames[frameIndex]; pet.needsDisplay = true
        pet.setAccessibilityLabel("洛琪希 · \(manifest.clips[state]?.label ?? state) · 第 \(frameIndex + 1) 帧")
    }
    func item(_ title: String, _ selector: Selector, key: String = "") -> NSMenuItem {
        let item = NSMenuItem(title: title, action: selector, keyEquivalent: key); item.target = self; return item
    }
    func makeMenu() -> NSMenu {
        let menu = NSMenu(); menu.delegate = self
        let label = NSMenuItem(title: "洛琪希 · 高清动画桌宠", action: nil, keyEquivalent: ""); label.isEnabled = false; menu.addItem(label)
        let pause = item(paused ? "继续动画" : "暂停动画", #selector(togglePause)); menu.addItem(pause)
        let auto = item("自动互动与视线跟随", #selector(toggleAutomatic)); auto.state = automatic ? .on : .off; menu.addItem(auto)
        let follow = item("跟随 Codex 任务", #selector(toggleCodex)); follow.state = followCodex ? .on : .off; menu.addItem(follow)
        let connection = NSMenuItem(title: codexLabel, action: nil, keyEquivalent: ""); connection.isEnabled = false; menu.addItem(connection)
        let actions = NSMenu()
        for name in stateOrder {
            let action = item(manifest.clips[name]!.label, #selector(playAction(_:))); action.representedObject = name
            action.state = state == name ? .on : .off; actions.addItem(action)
        }
        let actionParent = NSMenuItem(title: "播放动作", action: nil, keyEquivalent: ""); actionParent.submenu = actions; menu.addItem(actionParent)
        let sizes = NSMenu()
        for h in [300,400,500,560] {
            let size = item("\(h) 点", #selector(resizePet(_:))); size.tag = h; size.state = Int(height) == h ? .on : .off; sizes.addItem(size)
        }
        let sizeParent = NSMenuItem(title: "人物大小", action: nil, keyEquivalent: ""); sizeParent.submenu = sizes; menu.addItem(sizeParent)
        menu.addItem(item("移回主屏幕", #selector(resetPosition)))
        menu.addItem(.separator())
        let help = NSMenuItem(title: "单击挥手 · 双击跳跃 · 拖动移动", action: nil, keyEquivalent: ""); help.isEnabled = false; menu.addItem(help)
        menu.addItem(item("退出", #selector(quitApp), key: "q"))
        return menu
    }
    func menuWillOpen(_ menu: NSMenu) { menuOpen = true }
    func menuDidClose(_ menu: NSMenu) { menuOpen = false }
    func refreshMenu() { statusItem?.menu = makeMenu() }
    @objc func playAction(_ sender: NSMenuItem) { paused = false; manualUntil = ProcessInfo.processInfo.systemUptime + 6; select(sender.representedObject as! String) }
    @objc func toggleCodex() { followCodex.toggle(); if !followCodex { select("idle") }; refreshMenu() }
    func pollCodex() {
        let directory = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Library/Application Support/RoxyHD/codex-status")
        let now = Date().timeIntervalSince1970
        let files = (try? FileManager.default.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil)) ?? []
        let records: [(String, Double)] = files.filter { $0.pathExtension == "json" }.compactMap { file in
            guard let data = try? Data(contentsOf: file), let value = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                  let updated = value["updated"] as? Double, let state = value["state"] as? String,
                  now - updated >= -5, now - updated < 1800 else { return nil }
            return (state, updated)
        }
        let next = records.contains { $0.0 == "waiting" } ? "waiting" : records.contains { $0.0 == "running" } ? "running" : records.contains { $0.0 == "review" && now - $0.1 < 8 } ? "review" : nil
        let label = next == "running" ? "Codex：正在处理任务" : next == "waiting" ? "Codex：等待你的操作" : next == "review" ? "Codex：本轮已结束" : records.isEmpty ? "尚未收到 Codex 事件（或连接过期）" : "Codex：空闲"
        if codexState != next || codexLabel != label {
            let old = codexState; codexState = next; codexLabel = label
            if old != nil && next == nil && followCodex && ProcessInfo.processInfo.systemUptime > manualUntil { select("idle") }
            refreshMenu()
        }
    }
    @objc func togglePause() { paused.toggle(); refreshMenu() }
    @objc func toggleAutomatic() { automatic.toggle(); if !automatic && state.hasPrefix("look-") { select("idle") }; refreshMenu() }
    @objc func resizePet(_ sender: NSMenuItem) {
        height = CGFloat(sender.tag)
        var frame = panel.frame; frame.size = windowSize(); panel.setFrame(visiblePetFrame(frame), display: true)
        pet.needsDisplay = true
        settings.set(Double(height), forKey: "displayHeight"); refreshMenu()
    }
    @objc func resetPosition() {
        guard let area = NSScreen.screens.first?.visibleFrame else { return }
        panel.setFrameOrigin(NSPoint(x: area.maxX - panel.frame.width - 24, y: area.minY + 16))
        panel.orderFrontRegardless()
    }
    @objc func quitApp() { NSApp.terminate(nil) }
    func applicationWillTerminate(_ notification: Notification) {
        if let panel = panel { settings.set(NSStringFromPoint(panel.frame.origin), forKey: "position") }
    }
    func renderQA(directory: String) {
        try? FileManager.default.createDirectory(atPath: directory, withIntermediateDirectories: true)
        for name in stateOrder {
            state = name; frameIndex = 0; lastDraw = ""; redraw()
            guard let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int(ceil(pet.bounds.width * 2)),
                pixelsHigh: Int(ceil(pet.bounds.height * 2)), bitsPerSample: 8, samplesPerPixel: 4,
                hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0),
                let context = NSGraphicsContext(bitmapImageRep: bitmap) else { continue }
            NSGraphicsContext.saveGraphicsState(); NSGraphicsContext.current = context
            context.cgContext.scaleBy(x: 2, y: 2); pet.draw(pet.bounds); NSGraphicsContext.restoreGraphicsState()
            try? bitmap.representation(using: .png, properties: [:])?.write(to: URL(fileURLWithPath: directory).appendingPathComponent("\(name).png"))
        }
    }
}
let application = NSApplication.shared
let controller = PetController()
application.setActivationPolicy(.accessory)
application.delegate = controller
application.run()
