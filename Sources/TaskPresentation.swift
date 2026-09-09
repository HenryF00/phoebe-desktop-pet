import Cocoa

extension VoiceController {
    var unreadTasks: [[String:Any]] {
        let seen=UserDefaults.standard.dictionary(forKey:"blackboardReadRevisions") ?? [:]
        return delegatedTasks.filter {
            ["completed","failed","interrupted"].contains($0["status"] as? String ?? "") &&
            !($0["result"] as? String ?? "").isEmpty &&
            (seen[$0["id"] as? String ?? ""] as? Double) != ($0["updated"] as? Double ?? 0)
        }.sorted { ($0["updated"] as? Double ?? 0) > ($1["updated"] as? Double ?? 0) }
    }
    var taskBadgeState: String {
        if delegatedTasks.contains(where:{["queued","starting","running"].contains($0["status"] as? String ?? "")}) { return "running" }
        if delegatedTasks.contains(where:{$0["status"] as? String == "waiting"}) { return "waiting" }
        if !unreadTasks.isEmpty { return "unread" }
        return "none"
    }
    func markBoardRead(_ ident:String) {
        guard let job=delegatedTasks.first(where:{$0["id"] as? String == ident}) else { return }
        var seen=UserDefaults.standard.dictionary(forKey:"blackboardReadRevisions") ?? [:]
        seen[ident]=job["updated"] as? Double ?? 0
        UserDefaults.standard.set(seen,forKey:"blackboardReadRevisions")
    }
    func ensureBoardWindow() {
        if boardController != nil { return }
        let controller=TeachingBoardController()
        controller.onSpeak={ [weak self] ident in self?.requestBoardSpeech(ident) }
        controller.onRetry={ [weak self] ident in self?.prepareBoard(ident,force:true) }
        controller.onSettings={ [weak self] in self?.showSettings() }
        controller.onStop={ [weak self] in self?.stopBoardSpeech() }
        controller.onFollowUp={ [weak self] text in self?.submitBoardFollowUp(text) }
        controller.onRecordBegin={ [weak self] in self?.beginRecording() }
        controller.onRecordEnd={ [weak self] in self?.finishRecording(send:true) }
        controller.onLayout={ [weak self,weak controller] in
            guard let self=self,self.lectureMode,let controller=controller else{return}
            self.lecturePlacement?(controller.window)
        }
        controller.onClose={ [weak self] in self?.leaveLecture() }
        boardController=controller
    }
    func prepareBoard(_ ident:String,force:Bool=false) {
        launchWorker()
        var command:[String:Any]=["type":"board_prepare","task_id":ident,"force":force]
        if ident.hasPrefix("chat-"),let job=chatBoards[ident] { command["result"]=job["result"];command["title"]=job["title"] }
        sendCommand(command)
    }
    func openBoard(_ ident:String,automatic:Bool=false) {
        ensureBoardWindow();enterLecture()
        if let board=boards[ident] {
            autoAwaitingBoards.remove(ident);automaticBoards.removeAll { $0 == ident }
            if boardController?.board["task_id"] as? String != ident { stopBoardSpeech() }
            boardController?.show(board,activate:!automatic);markBoardRead(ident)
            if !automatic,let footer=boardController?.captions {
                footer.panel.makeKeyAndOrderFront(nil);footer.panel.makeFirstResponder(footer.input)
            }
            if preferences.boardSpeech { requestBoardSpeech(ident) }
        } else {
            requestedBoard=ident
            let job=delegatedTasks.first(where:{$0["id"] as? String == ident}) ?? chatBoards[ident] ?? [:]
            boardController?.show(["task_id":ident,"title":job["title"] as? String ?? "小黑板",
                "preparing":true,"takeaway":"","original":job["result"] as? String ?? ""],activate:!automatic)
            boardController?.captions.setInputEnabled(false)
            prepareBoard(ident)
        }
    }
    @objc func showTaskIndicator() {
        if let job=unreadTasks.first,let ident=job["id"] as? String {
            if job["presentation"] as? String == "brief" { showBriefResult(ident) } else { openBoard(ident) }
        }
        else { showTasks() }
    }
    @objc func explainCurrentReply() {
        guard !currentAnswer.isEmpty else { return }
        let id="chat-"+UUID().uuidString
        chatBoards[id]=["id":id,"title":String(currentUser.prefix(80)),"result":currentAnswer]
        openBoard(id)
    }
    func requestBoardSpeech(_ ident:String) {
        guard let board=boards[ident],!(board["narration"] as? [[String:String]] ?? []).isEmpty else { return }
        stopBoardSpeech();boardAudioDone=false;boardSpeechFailure=nil
        let token=UUID().uuidString;boardSpeechRequest=token;boardSpeechTask=ident
        boardController?.setBusy(true)
        launchWorker();sendCommand(["type":"board_speak","task_id":ident,"request_id":token])
    }
    func stopBoardSpeech() {
        boardSpeechRequest=nil;boardSpeechTask=nil;boardAudio=nil;boardAudioDone=false;currentLessonAudio=nil;boardSpeechFailure=nil
        boardController?.endLesson(completed:false)
        sendCommand(["type":"board_cancel"])
        if activeBoardSpeech { player?.stop();player=nil;activeBoardSpeech=false;activeIsNotice=false;playNext() }
    }
    /// Called on the main run loop; auto presentation never interrupts chat or another explanation.
    func presentationTick() {
        guard !generating,recorder==nil,player==nil,!streamPlayer.isBusy,boardSpeechRequest==nil else { return }
        if preferences.taskDelivery == "auto",let brief=pendingBriefTasks.first {
            pendingBriefTasks.removeFirst();showBriefResult(brief);return
        }
        guard boardController?.window.isVisible != true else { return }
        guard preferences.taskDelivery == "auto",let ident=automaticBoards.first else { return }
        automaticBoards.removeFirst();openBoard(ident,automatic:true)
    }
    func finishBoardIfIdle() {
        if boardAudioDone && boardAudioQueue.isEmpty && !activeBoardSpeech {
            boardSpeechRequest=nil;boardSpeechTask=nil;boardController?.endLesson(completed:boardSpeechFailure == nil)
            if let failure=boardSpeechFailure { boardController?.showError(failure) }
        }
    }
    func showBriefResult(_ ident:String) {
        guard let job=delegatedTasks.first(where:{$0["id"] as? String == ident}) else{return}
        noticeMessage=String((job["result"] as? String ?? "").prefix(600));renderReplyText();markBoardRead(ident)
    }
    func receiveBoard(_ event:[String:Any])->Bool {
        let type=event["type"] as? String ?? ""
        if type=="board_preparing" {
            if event["task_id"] as? String == boardController?.board["task_id"] as? String {
                boardController?.setBusy(true)
            }
            return true
        }
        if type=="board_ready",let board=event["board"] as? [String:Any],let ident=board["task_id"] as? String {
            if let job=delegatedTasks.first(where:{$0["id"] as? String == ident}),
                job["result"] as? String != board["original"] as? String { return true }
            guard board["subtitle_language"] as? String == preferences.subtitleLanguage,
                  board["voice_language"] as? String == preferences.voiceLanguage else {
                if requestedBoard == ident || autoAwaitingBoards.contains(ident) { prepareBoard(ident) }
                return true
            }
            boards[ident]=board
            if requestedBoard==ident || boardController?.board["task_id"] as? String==ident {
                boardController?.captions.setInputEnabled(true)
                if lectureDelegatedTask==ident { lectureDelegatedTask=nil }
            }
            if requestedBoard==ident {
                requestedBoard=nil;openBoard(ident)
            } else if autoAwaitingBoards.remove(ident) != nil && preferences.taskDelivery == "auto" {
                if !automaticBoards.contains(ident) { automaticBoards.append(ident) }
                presentationTick()
            } else if boardController?.board["task_id"] as? String == ident, boardController?.window.isVisible == true {
                boardController?.show(board,activate:false)
            }
            return true
        }
        if type=="board_audio" {
            guard event["request_id"] as? String == boardSpeechRequest,
                  let encoded=event["data"] as? String,let data=Data(base64Encoded:encoded) else { return true }
            boardAudioQueue.append(LessonAudio(data:data,caption:event["subtitle"] as? String ?? "",focus:event["focus"] as? String ?? "takeaway",gesture:event["gesture"] as? String ?? "explain",index:event["index"] as? Int ?? 0))
            playNext();return true
        }
        if type=="board_audio_done" {
            guard event["request_id"] as? String == boardSpeechRequest else { return true }
            boardAudioDone=true;finishBoardIfIdle();return true
        }
        if type=="task_brief_audio" {
            if preferences.taskDelivery == "auto",let ident=event["task_id"] as? String,let job=delegatedTasks.first(where:{$0["id"] as? String == ident}),let encoded=event["data"] as? String,let data=Data(base64Encoded:encoded) {
                clips.append((data,true,String((job["result"] as? String ?? "").prefix(600))));playNext()
            };return true
        }
        if type=="board_error" {
            if let request=event["request_id"] as? String,request != boardSpeechRequest { return true }
            if let pending=requestedBoard,event["task_id"] as? String != pending { return true }
            if let request=event["request_id"] as? String,request == boardSpeechRequest {
                boardAudioDone=true;boardSpeechFailure=event["message"] as? String;finishBoardIfIdle()
            }
            if event["task_id"] as? String == boardController?.board["task_id"] as? String || event["task_id"] as? String==requestedBoard {
                boardController?.captions.setInputEnabled(true)
                if event["task_id"] as? String==requestedBoard { requestedBoard=nil }
                boardController?.showError(event["message"] as? String ?? "讲解暂时不可用。")
            }
            return true
        }
        return false
    }
}
