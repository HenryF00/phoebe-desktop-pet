import Cocoa

extension VoiceController {
    func showInputNotice(_ text:String) {
        phase.stringValue=text
        if lectureMode { boardController?.showError(text) } else { showSubtitle(text) }
    }
    func enterLecture() {
        if !lectureMode { lecturePreviousComposer=composerShown }
        lectureMode=true;composerShown=false;bubbleShown=false
        suggestions.hide();window.orderOut(nil);bubble.orderOut(nil)
        if let board=boardController,board.window.isVisible { lecturePlacement?(board.window) }
    }
    func leaveLecture() {
        guard lectureMode else{return}
        let dismissed=[requestedBoard,lectureDelegatedTask,boardController?.board["task_id"] as? String].compactMap{$0}
        autoAwaitingBoards.subtract(dismissed)
        automaticBoards.removeAll { dismissed.contains($0) }
        requestedBoard=nil;lectureDelegatedTask=nil
        if recorder != nil { finishRecording(send:false) }
        cancelChat();lectureCurrentTurn=false
        boardController?.captions.setInputEnabled(true)
        lectureMode=false;lecturePlacement?(nil)
        if lecturePreviousComposer { show() }
    }
    func submitBoardFollowUp(_ text:String) {
        let question=text.trimmingCharacters(in:.whitespacesAndNewlines)
        guard lectureMode,!question.isEmpty,!generating,requestedBoard==nil else{return}
        guard question.count<=4000 else { boardController?.showError("问题太长了，请分开说。");return }
        if question.range(of:"^/(聊天|chat)(?=\\s|[:：]|$)",options:[.regularExpression,.caseInsensitive]) != nil {
            boardController?.window.close();show()
        }
        submit(["text":question])
    }
}
