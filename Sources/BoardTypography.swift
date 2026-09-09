import Cocoa
import CoreText

/// A bundled, rounded handwritten face. Registration is private to Roxy's process.
enum BoardTypography {
    static let fontName = "Xiaolai"
    private static let registered: Bool = {
        var candidates: [URL] = []
        if let resources=Bundle.main.resourceURL { candidates.append(resources.appendingPathComponent("fonts/Xiaolai-Regular.ttf")) }
        if let marker=Bundle.main.url(forResource:"project-root",withExtension:"txt"),
           let value=try? String(contentsOf:marker,encoding:.utf8) {
            candidates.append(URL(fileURLWithPath:value.trimmingCharacters(in:.whitespacesAndNewlines)).appendingPathComponent("assets/fonts/Xiaolai-Regular.ttf"))
        }
        // Source-tree harnesses use the identical font without installing it in macOS.
        candidates.append(URL(fileURLWithPath:#filePath).deletingLastPathComponent().deletingLastPathComponent().appendingPathComponent("assets/fonts/Xiaolai-Regular.ttf"))
        for url in candidates where FileManager.default.fileExists(atPath:url.path) {
            CTFontManagerRegisterFontsForURL(url as CFURL,.process,nil)
            if NSFont(name:fontName,size:18) != nil { return true }
        }
        return false
    }()
    static func font(_ size:CGFloat,weight:NSFont.Weight = .regular)->NSFont {
        _ = registered
        if let face=NSFont(name:fontName,size:size) { return face }
        let system=NSFont.systemFont(ofSize:size,weight:weight)
        return system.fontDescriptor.withDesign(.rounded).flatMap { NSFont(descriptor:$0,size:size) } ?? system
    }
}
