import Cocoa
let root = URL(fileURLWithPath: CommandLine.arguments[1])
func load(_ path:String) -> NSBitmapImageRep { NSBitmapImageRep(data: try! Data(contentsOf:root.appendingPathComponent(path)))! }
func pixel(_ im:NSBitmapImageRep,_ x:Int,_ y:Int)->[Int] { var p=[Int](repeating:0,count:4);im.getPixel(&p,atX:x,y:y);if !im.hasAlpha {p[3]=255};return p }
var report:[String:Any]=[:]
let samplePoints=["hat":[700,165],"cape":[850,590]]
for (key,path) in [("master","assets/pet/frames/master.png"),("old_up","qa/teaching-lighting/before-pointer-up.png"),("fixed_original_up","qa/teaching-lighting/original-source-fixed-export-up.png"),("fixed_original_level","qa/teaching-lighting/original-source-fixed-export-level.png"),("fixed_edited_up","assets/pet/frames/teaching-pointer-up.png"),("fixed_edited_level","assets/pet/frames/teaching-pointer-level.png")] {
 let im=load(path);var samples:[String:[Int]]=[:]
 for (name,p) in samplePoints {samples[name]=Array(pixel(im,p[0],p[1]).prefix(3))}
 report[key]=samples
}
for name in ["pointer-up","pointer-level"] {
 let im=load("assets/pet/frames/teaching-\(name).png")
 let mirrored=load("assets/pet/frames/teaching-\(name)-right.png")
 let source=load("assets/pet/teaching-source/\(name).png")
 assert(im.pixelsWide==1440 && im.pixelsHigh==1400 && im.hasAlpha)
 var transparent=0, partial=0, opaque=0, strayMagenta=0, mirrorMax=0
 var minX=1440,minY=1400,maxX=0,maxY=0
 for y in 0..<1400 { for x in 0..<1440 {
   let p=pixel(im,x,y),m=pixel(mirrored,1439-x,y)
   for c in 0..<4 { mirrorMax=max(mirrorMax,abs(p[c]-m[c])) }
   if p[3]==0 {transparent+=1} else if p[3]<255 {partial+=1} else {opaque+=1}
   if p[3]>128 {minX=min(minX,x);minY=min(minY,y);maxX=max(maxX,x);maxY=max(maxY,y)}
   if p[3]>230 && min(p[0],p[2])-p[1]>75 {strayMagenta+=1}
 }}
 var colorErrors:[Double]=[]
 for y in stride(from:2,to:source.pixelsHigh-2,by:7) {for x in stride(from:2,to:source.pixelsWide-2,by:7) {
   let p=pixel(source,x,y)
   if min(p[0],p[2])-p[1]>15 {continue}
   var flat=true
   for dy in -1...1 {for dx in -1...1 {let n=pixel(source,x+dx,y+dy);if (0..<3).contains(where:{abs(n[$0]-p[$0])>4}) {flat=false}}}
   if !flat {continue}
   let tx=min(1439,Int((Double(x)+0.5)*1440/Double(source.pixelsWide)))
   let ty=min(1399,Int((Double(y)+0.5)*1400/Double(source.pixelsHigh)))
   let q=pixel(im,tx,ty);if q[3]<250 {continue}
   colorErrors.append((0..<3).map{Double(abs(q[$0]-p[$0]))}.reduce(0,+)/3)
 }}
 colorErrors.sort();let median=colorErrors[colorErrors.count/2]
 assert(strayMagenta==0,"Magenta background leaked")
 assert(transparent>1_000_000,"Expected transparent sprite background")
 assert(mirrorMax<=2,"Directional frames must mirror exactly apart from rounding")
 assert(median<1,"Exporter changed flat sRGB colors")
 report[name] = ["width":1440,"height":1400,"transparent_pixels":transparent,"partial_alpha_pixels":partial,"opaque_pixels":opaque,"stray_opaque_magenta":strayMagenta,"mirrored_max_channel_error":mirrorMax,"opaque_bounds":[minX,minY,maxX,maxY],"flat_color_sample_count":colorErrors.count,"median_source_to_export_rgb_error":median]
}
let data=try JSONSerialization.data(withJSONObject:report,options:[.prettyPrinted,.sortedKeys])
try data.write(to:root.appendingPathComponent("qa/teaching-lighting/validation.json"))
print(String(data:data,encoding:.utf8)!)
