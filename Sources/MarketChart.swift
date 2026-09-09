import Cocoa

/// Offline rendering of validated OHLC rows; never parses executable chart markup.
enum MarketChartDrawing {
    static func image(_ market:[String:Any],days:Int,candles:Bool,width:CGFloat=700)->NSImage {
        let rows=Array((market["rows"] as? [[String:Any]] ?? []).suffix(days))
        let height:CGFloat=365
        return NSImage(size:NSSize(width:width,height:height),flipped:true) { rect in
            guard rows.count>=2 else{return true}
            let ink=NSColor(calibratedRed:0.91,green:0.94,blue:0.88,alpha:1)
            let muted=ink.withAlphaComponent(0.70)
            let up=NSColor(calibratedRed:0.98,green:0.43,blue:0.42,alpha:1)
            let down=NSColor(calibratedRed:0.34,green:0.80,blue:0.62,alpha:1)
            func value(_ row:[String:Any],_ key:String)->Double { (row[key] as? NSNumber)?.doubleValue ?? 0 }
            func text(_ string:String,_ x:CGFloat,_ y:CGFloat,_ size:CGFloat,_ color:NSColor,_ bold:Bool=false) {
                string.draw(at:NSPoint(x:x,y:y),withAttributes:[.font:NSFont.monospacedDigitSystemFont(ofSize:size,weight:bold ? .semibold:.regular),.foregroundColor:color])
            }
            let first=rows.first!,last=rows.last!,close=value(last,"close"),start=value(first,"close")
            let change=start>0 ? (close/start-1)*100:0
            let tint=change>=0 ? up:down
            text(market["symbol"] as? String ?? "",0,0,17,ink,true)
            text(String(format:"%.2f",close),0,28,32,tint,true)
            text(String(format:"%+.2f%%  区间",change),135,42,13,tint)
            text("USD · 日线 · "+(market["as_of"] as? String ?? ""),0,70,11,muted)
            let left:CGFloat=0,top:CGFloat=104,plotWidth=width-64,plotHeight:CGFloat=176
            let low=rows.map { value($0,"low") }.min()!,high=rows.map { value($0,"high") }.max()!
            let spread=max(high-low,high*0.015),minY=low-spread*0.08,maxY=high+spread*0.08
            func y(_ p:Double)->CGFloat { top+CGFloat((maxY-p)/(maxY-minY))*plotHeight }
            let dx=plotWidth/CGFloat(rows.count)
            for i in 0...4 {
                let amount=minY+(maxY-minY)*Double(i)/4,py=y(amount)
                let grid=NSBezierPath();grid.move(to:NSPoint(x:left,y:py));grid.line(to:NSPoint(x:plotWidth,y:py));grid.lineWidth=0.5
                ink.withAlphaComponent(0.12).setStroke();grid.stroke()
                text(String(format:"%.2f",amount),plotWidth+8,py-6,10,muted)
            }
            let maxVolume=rows.map { value($0,"volume") }.max() ?? 1
            let line=NSBezierPath()
            for (index,row) in rows.enumerated() {
                let x=left+dx*(CGFloat(index)+0.5),p=NSPoint(x:x,y:y(value(row,"close")))
                if index==0 { line.move(to:p) } else { line.line(to:p) }
                if candles {
                    let positive=value(row,"close")>=value(row,"open"),color=positive ? up:down
                    let wick=NSBezierPath();wick.move(to:NSPoint(x:x,y:y(value(row,"high"))));wick.line(to:NSPoint(x:x,y:y(value(row,"low"))));wick.lineWidth=1
                    color.setStroke();wick.stroke();color.setFill()
                    let a=y(value(row,"open")),b=y(value(row,"close"))
                    NSRect(x:x-max(1,dx*0.62)/2,y:min(a,b),width:max(1,dx*0.62),height:max(1,abs(a-b))).fill()
                }
                let volumeHeight=CGFloat(value(row,"volume")/max(1,maxVolume))*34
                (value(row,"close")>=value(row,"open") ? up:down).withAlphaComponent(0.48).setFill()
                NSRect(x:x-dx*0.31,y:325-volumeHeight,width:max(1,dx*0.62),height:volumeHeight).fill()
            }
            if !candles {
                let fill=line.copy() as! NSBezierPath
                fill.line(to:NSPoint(x:plotWidth-dx/2,y:top+plotHeight));fill.line(to:NSPoint(x:dx/2,y:top+plotHeight));fill.close()
                NSGradient(starting:tint.withAlphaComponent(0.25),ending:tint.withAlphaComponent(0.015))!.draw(in:fill,angle:90)
                line.lineWidth=2.2;line.lineJoinStyle = .round;tint.setStroke();line.stroke()
            }
            text("成交量",0,281,9,muted)
            text(first["date"] as? String ?? "",0,331,10,muted)
            text(last["date"] as? String ?? "",plotWidth-72,331,10,muted)
            text("Nasdaq · 历史日线 / 未自行复权 · 红涨绿跌",0,351,10,muted)
            return true
        }
    }
}
