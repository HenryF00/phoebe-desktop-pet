import Cocoa
let root=URL(fileURLWithPath:"/Users/ren/Documents/GitHub/Roxy")
let paths=["assets/pet/frames/master.png","qa/teaching-lighting/before-pointer-up.png","assets/pet/frames/teaching-pointer-up.png","qa/teaching-lighting/before-pointer-level.png","assets/pet/frames/teaching-pointer-level.png"]
let labels=["Original master","Before: diagonal","Corrected: diagonal","Before: level","Corrected: level"]
let out=NSBitmapImageRep(bitmapDataPlanes:nil,pixelsWide:2000,pixelsHigh:850,bitsPerSample:8,samplesPerPixel:4,hasAlpha:true,isPlanar:false,colorSpaceName:.deviceRGB,bytesPerRow:0,bitsPerPixel:0)!
NSGraphicsContext.saveGraphicsState();NSGraphicsContext.current=NSGraphicsContext(bitmapImageRep:out)
NSColor(calibratedWhite:0.17,alpha:1).setFill();NSRect(x:0,y:0,width:2000,height:850).fill()
for i in 0..<paths.count {
  let img=NSImage(contentsOf:root.appendingPathComponent(paths[i]))!
  img.draw(in:NSRect(x:CGFloat(i)*400,y:400,width:400,height:389),from:.zero,operation:.sourceOver,fraction:1)
  let text=labels[i] as NSString
  text.draw(at:NSPoint(x:CGFloat(i)*400+20,y:815),withAttributes:[.font:NSFont.systemFont(ofSize:19),.foregroundColor:NSColor.white])
  // Enlarged head and shoulders: same source crop across aligned runtime canvases.
  img.draw(in:NSRect(x:CGFloat(i)*400+5,y:20,width:390,height:365),from:NSRect(x:445,y:780,width:520,height:485),operation:.sourceOver,fraction:1)
}
NSGraphicsContext.restoreGraphicsState()
try out.representation(using:.png,properties:[:])!.write(to:root.appendingPathComponent("qa/teaching-lighting/comparison.png"))
print("comparison.png")
