import AVFoundation
import Foundation

/// Main-thread coordinator: buffers share one audio engine/node, without a new
/// player (and device setup gap) for every network fragment.
final class StreamingAudioPlayer {
    final class Segment {
        let id: String
        let subtitle: String
        var buffers: [AVAudioPCMBuffer] = []
        var ended = false
        var pending = 0
        var began = false
        init(_ id: String, _ subtitle: String) { self.id = id; self.subtitle = subtitle }
    }
    let engine = AVAudioEngine()
    let node = AVAudioPlayerNode()
    let format = AVAudioFormat(standardFormatWithSampleRate: 32000, channels: 1)!
    var segments: [Segment] = []
    var generation = 0
    var suspended = false { didSet { if !suspended { pump() } } }
    var onBegan: ((String) -> Void)?
    var onIdle: (() -> Void)?
    var onError: (() -> Void)?
    var isBusy: Bool { !segments.isEmpty }
    init() {
        engine.attach(node)
        engine.connect(node, to: engine.mainMixerNode, format: format)
    }
    func start(_ id: String, subtitle: String, sampleRate: Int) {
        guard sampleRate == 32000, !segments.contains(where: { $0.id == id }) else { return }
        segments.append(Segment(id, subtitle))
    }
    func append(_ id: String, data: Data) {
        guard let segment = segments.first(where: { $0.id == id }), !segment.ended,
              !data.isEmpty, data.count % 2 == 0 else { return }
        let frames = data.count / 2
        guard let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: AVAudioFrameCount(frames)),
              let samples = buffer.floatChannelData?[0] else { return }
        buffer.frameLength = AVAudioFrameCount(frames)
        data.withUnsafeBytes { raw in
            let bytes = raw.bindMemory(to: UInt8.self)
            for i in 0..<frames {
                let value = UInt16(bytes[i * 2]) | UInt16(bytes[i * 2 + 1]) << 8
                samples[i] = Float(Int16(bitPattern: value)) / 32768.0
            }
        }
        segment.buffers.append(buffer); pump()
    }
    func end(_ id: String) {
        segments.first(where: { $0.id == id })?.ended = true; pump()
    }
    func pump() {
        guard !suspended, let segment = segments.first else { return }
        if segment.ended && segment.pending == 0 && segment.buffers.isEmpty {
            segments.removeFirst()
            if segments.isEmpty { node.stop(); onIdle?() } else { pump() }
            return
        }
        guard !segment.buffers.isEmpty else { return }
        do { if !engine.isRunning { try engine.start() } }
        catch { stop(); onError?(); return }
        let epoch = generation
        while !segment.buffers.isEmpty {
            let buffer = segment.buffers.removeFirst(); segment.pending += 1
            node.scheduleBuffer(buffer, completionCallbackType: .dataPlayedBack) { [weak self, weak segment] _ in
                DispatchQueue.main.async {
                    guard let self = self, let segment = segment, self.generation == epoch else { return }
                    segment.pending -= 1; self.pump()
                }
            }
        }
        if !segment.began { segment.began = true; onBegan?(segment.subtitle) }
        if !node.isPlaying { node.play() }
    }
    func finishOpenSegments() {
        for segment in segments { segment.ended = true }; pump()
    }
    func stop() {
        generation += 1; node.stop(); engine.stop(); segments.removeAll()
    }
}
