import Cocoa

final class HistoryTextField: NSTextField {
    var onDoubleClick: (() -> Void)?
    override func mouseDown(with event: NSEvent) {
        if event.clickCount == 2 { onDoubleClick?() } else { super.mouseDown(with:event) }
    }
}

final class ChatHistoryController {
    let window: NSWindow
    let transcript: NSTextView
    init(transcript: NSTextView) {
        self.transcript=transcript
        window=NSWindow(contentRect:NSRect(x:0,y:0,width:520,height:600),
                        styleMask:[.titled,.closable,.resizable],backing:.buffered,defer:false)
        window.title="洛琪希 · 聊天记录（本机）";window.isReleasedWhenClosed=false
        window.minSize=NSSize(width:360,height:300);window.center()
        let scroll=NSScrollView(frame:window.contentView!.bounds)
        scroll.autoresizingMask=[.width,.height];scroll.hasVerticalScroller=true;scroll.borderType = .noBorder
        transcript.frame=scroll.contentView.bounds
        transcript.isEditable=false;transcript.isSelectable=true
        transcript.isVerticallyResizable=true;transcript.isHorizontallyResizable=false
        transcript.autoresizingMask = [.width]
        transcript.textContainerInset=NSSize(width:20,height:18)
        transcript.textContainer?.widthTracksTextView=true
        transcript.textContainer?.containerSize=NSSize(width:scroll.contentSize.width,height:CGFloat.greatestFiniteMagnitude)
        transcript.minSize=NSSize(width:0,height:scroll.contentSize.height)
        transcript.maxSize=NSSize(width:CGFloat.greatestFiniteMagnitude,height:CGFloat.greatestFiniteMagnitude)
        transcript.backgroundColor = .textBackgroundColor
        transcript.setAccessibilityLabel("聊天记录，可选择复制")
        scroll.documentView=transcript;window.contentView!.addSubview(scroll)
    }
    func show() { window.makeKeyAndOrderFront(nil);transcript.scrollToEndOfDocument(nil);NSApp.activate(ignoringOtherApps:true) }
}
