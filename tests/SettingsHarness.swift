import Cocoa
let app=NSApplication.shared;app.setActivationPolicy(.accessory)
let ui=ChatSettingsController()
var captured:ChatPreferences?;var savedKey=false
ui.persist = { captured=$0 }
ui.saveKey = { value in savedKey = value == "test-only-not-a-real-key" }
ui.provider.selectItem(at:1);ui.caption.selectItem(at:1);ui.speech.selectItem(at:1);ui.providerChanged()
assert(ui.codexRow.isHidden && !ui.deepseekRows.isHidden)
ui.key.stringValue="test-only-not-a-real-key"
assert(ui.saveSettings() && savedKey && ui.key.stringValue.isEmpty)
assert(captured?.provider == "deepseek" && captured?.subtitleLanguage == "en" && captured?.voiceLanguage == "zh")
assert(captured?.payload["api_key"] == nil)
let root=URL(fileURLWithPath:CommandLine.arguments[1]).appendingPathComponent("qa/settings-ui")
try! FileManager.default.createDirectory(at:root,withIntermediateDirectories:true)
for mode in [NSAppearance.Name.aqua,.darkAqua] {
 for index in [0,1] {
  ui.provider.selectItem(at:index);ui.providerChanged();ui.window.appearance=NSAppearance(named:mode)
  ui.window.contentView!.layoutSubtreeIfNeeded()
  let view=ui.window.contentView!;let bitmap=view.bitmapImageRepForCachingDisplay(in:view.bounds)!
  ui.window.appearance!.performAsCurrentDrawingAppearance { view.displayIfNeeded();view.cacheDisplay(in:view.bounds,to:bitmap) }
  try! bitmap.representation(using:.png,properties:[:])!.write(to:root.appendingPathComponent("settings-\(index)-\(mode.rawValue).png"))
 }
}
print("PASS: provider visibility, independent languages, save, secret exclusion; Keychain writes stubbed")
