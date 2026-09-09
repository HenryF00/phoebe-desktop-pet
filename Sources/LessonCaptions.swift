import Cocoa

/// The lesson's persistent conversation shelf, outside the scrolling board.
final class LessonCaptions: NSObject, NSWindowDelegate, NSTextFieldDelegate {
    let panel=CompanionPanel(contentRect:NSRect(x:0,y:0,width:700,height:140),styleMask:[.borderless,.nonactivatingPanel],backing:.buffered,defer:false)
    let label=NSTextField(wrappingLabelWithString:"")
    let userText=NSTextField(wrappingLabelWithString:"")
    let input=NSTextField(string:"")
    let mic=TalkButton(title:"",target:nil,action:nil)
    let editor=ComposerEditor()
    var onSubmit:((String)->Void)?
    var onRecordBegin:(()->Void)?
    var onRecordEnd:(()->Void)?
    var onLayout:(()->Void)?
    weak var boardWindow:NSWindow?
    var positioning=false
    private var stack:NSStackView!
    override init() {
        super.init()
        panel.title="洛琪希 · 课堂对话";panel.acceptsKeyboard=true;panel.delegate=self
        panel.isOpaque=false;panel.backgroundColor = .clear;panel.hasShadow=false;panel.isReleasedWhenClosed=false
        panel.hidesOnDeactivate=false;panel.collectionBehavior=[.fullScreenAuxiliary]
        panel.appearance=NSAppearance(named:.darkAqua)
        let glass=CompanionGlass(frame:panel.contentView!.bounds);panel.contentView=glass
        label.font=BoardTypography.font(21);label.textColor=NSColor(calibratedRed:0.94,green:0.96,blue:0.92,alpha:1)
        label.alignment = .center;label.isSelectable=true;label.maximumNumberOfLines=0;label.isHidden=true
        label.setAccessibilityLabel("授课字幕，与当前播放段落同步")
        userText.font = .systemFont(ofSize:13,weight:.regular);userText.textColor = .secondaryLabelColor
        userText.maximumNumberOfLines=2;userText.isSelectable=true;userText.isHidden=true
        userText.setAccessibilityLabel("你上一轮的课堂提问")
        input.font=BoardTypography.font(19);input.textColor = .labelColor;input.drawsBackground=false
        input.isBordered=false;input.focusRingType = .none;input.placeholderString=nil
        input.delegate=self;input.target=self;input.action=#selector(submit)
        input.setAccessibilityLabel("继续向洛琪希提问")
        editor.isFieldEditor=true;editor.drawsBackground=false;editor.font=input.font;editor.textColor=input.textColor
        mic.image=NSImage(systemSymbolName:"mic",accessibilityDescription:"按住说话")
        mic.imagePosition = .imageOnly;mic.isBordered=false;mic.contentTintColor = .secondaryLabelColor
        mic.toolTip="按住说话，松开发送";mic.setAccessibilityLabel("按住说话，松开发送")
        mic.pressed = { [weak self] in self?.onRecordBegin?() }
        mic.released = { [weak self] in self?.onRecordEnd?() }
        mic.widthAnchor.constraint(equalToConstant:36).isActive=true;mic.heightAnchor.constraint(equalToConstant:36).isActive=true
        let entry=NSStackView(views:[input,mic]);entry.orientation = .horizontal;entry.alignment = .centerY;entry.spacing=12
        input.setContentHuggingPriority(.defaultLow,for:.horizontal)
        input.setContentCompressionResistancePriority(.defaultLow,for:.horizontal)
        let separator=NSView();separator.wantsLayer=true;separator.layer?.backgroundColor=NSColor.white.withAlphaComponent(0.08).cgColor
        separator.heightAnchor.constraint(equalToConstant:1).isActive=true
        stack=NSStackView(views:[userText,label,separator,entry]);stack.orientation = .vertical;stack.alignment = .leading;stack.spacing=10
        stack.translatesAutoresizingMaskIntoConstraints=false;glass.addSubview(stack)
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo:glass.leadingAnchor,constant:26),
            stack.trailingAnchor.constraint(equalTo:glass.trailingAnchor,constant:-26),
            stack.topAnchor.constraint(equalTo:glass.topAnchor,constant:14),
            stack.bottomAnchor.constraint(equalTo:glass.bottomAnchor,constant:-12)])
        for view in stack.arrangedSubviews { view.widthAnchor.constraint(equalTo:stack.widthAnchor).isActive=true }
    }
    func setUserText(_ text:String) {
        userText.stringValue=text;userText.isHidden=text.isEmpty
        if let board=boardWindow,board.isVisible { layout(below:board) }
    }
    func setInputEnabled(_ enabled:Bool) {
        input.isEnabled=enabled;mic.isEnabled=enabled
        input.alphaValue=enabled ? 1:0.55;mic.alphaValue=enabled ? 1:0.55
    }
    @objc func submit(_ sender:Any?=nil) {
        guard input.isEnabled else { return }
        let text=(input.currentEditor()?.string ?? input.stringValue).trimmingCharacters(in:.whitespacesAndNewlines)
        guard !text.isEmpty else { return }
        setUserText(text);input.stringValue="";editor.string=""
        onSubmit?(text)
    }
    func control(_ control:NSControl,textView:NSTextView,doCommandBy selector:Selector)->Bool {
        guard control === input,selector == #selector(NSResponder.insertNewline(_:)),!textView.hasMarkedText() else { return false }
        input.stringValue=textView.string;submit();return true
    }
    func windowWillReturnFieldEditor(_ sender:NSWindow,to client:Any?)->Any? {
        if let field=client as? NSTextField,field === input { return editor };return nil
    }
    func show(_ text:String,below board:NSWindow) {
        boardWindow=board;label.stringValue=text;label.isHidden=text.isEmpty
        guard board.isVisible,!board.isMiniaturized else { hide();return }
        layout(below:board)
        if panel.parent !== board { board.addChildWindow(panel,ordered:.above) }
        panel.orderFrontRegardless()
    }
    func layout(below board:NSWindow) {
        guard !positioning else{return}
        positioning=true;defer{positioning=false;onLayout?()}
        let screen=board.screen?.visibleFrame ?? NSScreen.main?.visibleFrame ?? board.frame
        // Leave a right-hand teaching lane for Roxy when the screen can accommodate it.
        let rail:CGFloat=screen.width >= 1000 ? 245:0
        var frame=board.frame
        frame.size.width=min(frame.width,max(600,screen.width-rail-24))
        let width=frame.width
        func textHeight(_ field:NSTextField)->CGFloat {
            guard !field.isHidden else{return 0}
            field.preferredMaxLayoutWidth=width-52
            let rect=(field.stringValue as NSString).boundingRect(with:NSSize(width:width-52,height:1000),
                options:[.usesLineFragmentOrigin,.usesFontLeading],attributes:[.font:field.font!])
            // NSTextField's cell includes its own baseline/insets beyond NSString's glyph box.
            let cellHeight=field.cell?.cellSize(forBounds:NSRect(x:0,y:0,width:width-52,height:1000)).height ?? 0
            return ceil(max(rect.height,cellHeight))
        }
        let captionHeight=textHeight(label)
        let previousHeight=min(34,textHeight(userText))
        let height:CGFloat=81+(captionHeight>0 ? captionHeight+10:0)+(previousHeight>0 ? previousHeight+10:0)
        let gap:CGFloat=8
        frame.size.height=min(frame.height,screen.height-height-gap-8)
        frame.origin.x=max(screen.minX+8,min(frame.minX,screen.maxX-frame.width-rail-8))
        frame.origin.y=max(screen.minY+height+gap+4,min(frame.minY,screen.maxY-frame.height))
        if frame != board.frame { board.setFrame(frame,display:true) }
        panel.setFrame(NSRect(x:frame.minX,y:frame.minY-gap-height,width:width,height:height),display:true)
    }
    func hide() { panel.parent?.removeChildWindow(panel);panel.orderOut(nil) }
}
