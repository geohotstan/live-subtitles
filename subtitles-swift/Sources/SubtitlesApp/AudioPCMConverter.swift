@preconcurrency import AVFoundation
import CoreMedia

final class AudioPCMConverter {
    private let outputFormat: AVAudioFormat
    private let gain: Float
    private var converter: AVAudioConverter?
    private var inputFormat: AVAudioFormat?
    private let lock = NSLock()

    init(outputSampleRate: Double, outputChannels: AVAudioChannelCount, gain: Float) {
        self.outputFormat = AVAudioFormat(standardFormatWithSampleRate: outputSampleRate, channels: outputChannels)!
        self.gain = gain
    }

    func convert(sampleBuffer: CMSampleBuffer) -> AVAudioPCMBuffer? {
        guard CMSampleBufferDataIsReady(sampleBuffer) else { return nil }
        guard let pcmBuffer = makePCMBuffer(from: sampleBuffer) else { return nil }
        return resampleIfNeeded(pcmBuffer)
    }

    private func resampleIfNeeded(_ buffer: AVAudioPCMBuffer) -> AVAudioPCMBuffer? {
        if buffer.format.isEqual(outputFormat) {
            applyGain(to: buffer)
            return buffer
        }

        lock.lock()
        defer { lock.unlock() }

        if converter == nil || inputFormat == nil || !(inputFormat!.isEqual(buffer.format)) {
            inputFormat = buffer.format
            converter = AVAudioConverter(from: buffer.format, to: outputFormat)
        }

        guard let converter else { return nil }

        let ratio = outputFormat.sampleRate / buffer.format.sampleRate
        let frameCapacity = AVAudioFrameCount(Double(buffer.frameLength) * ratio) + 1
        guard let outputBuffer = AVAudioPCMBuffer(pcmFormat: outputFormat, frameCapacity: frameCapacity) else {
            return nil
        }

        var error: NSError?
        let inputBlock: AVAudioConverterInputBlock = { _, outStatus in
            outStatus.pointee = .haveData
            return buffer
        }
        converter.convert(to: outputBuffer, error: &error, withInputFrom: inputBlock)

        if error != nil {
            return nil
        }
        applyGain(to: outputBuffer)
        return outputBuffer
    }

    private func makePCMBuffer(from sampleBuffer: CMSampleBuffer) -> AVAudioPCMBuffer? {
        guard let formatDescription = CMSampleBufferGetFormatDescription(sampleBuffer) else { return nil }
        guard let asbd = CMAudioFormatDescriptionGetStreamBasicDescription(formatDescription) else { return nil }
        guard let format = AVAudioFormat(streamDescription: asbd) else { return nil }

        let frameCount = AVAudioFrameCount(CMSampleBufferGetNumSamples(sampleBuffer))
        guard let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: frameCount) else { return nil }
        buffer.frameLength = frameCount

        var blockBuffer: CMBlockBuffer?
        var bufferListSize = 0
        CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(
            sampleBuffer,
            bufferListSizeNeededOut: &bufferListSize,
            bufferListOut: nil,
            bufferListSize: 0,
            blockBufferAllocator: kCFAllocatorDefault,
            blockBufferMemoryAllocator: kCFAllocatorDefault,
            flags: 0,
            blockBufferOut: nil
        )

        let audioBufferListPointer = UnsafeMutableRawPointer.allocate(
            byteCount: bufferListSize,
            alignment: MemoryLayout<AudioBufferList>.alignment
        )
        defer { audioBufferListPointer.deallocate() }

        let audioBufferList = audioBufferListPointer.bindMemory(to: AudioBufferList.self, capacity: 1)

        let status = CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(
            sampleBuffer,
            bufferListSizeNeededOut: nil,
            bufferListOut: audioBufferList,
            bufferListSize: bufferListSize,
            blockBufferAllocator: kCFAllocatorDefault,
            blockBufferMemoryAllocator: kCFAllocatorDefault,
            flags: 0,
            blockBufferOut: &blockBuffer
        )

        guard status == noErr else { return nil }

        let sourceBuffers = UnsafeMutableAudioBufferListPointer(audioBufferList)
        let destinationBuffers = UnsafeMutableAudioBufferListPointer(buffer.mutableAudioBufferList)

        let count = min(sourceBuffers.count, destinationBuffers.count)
        for index in 0..<count {
            let source = sourceBuffers[index]
            let destination = destinationBuffers[index]
            guard let sourceData = source.mData, let destinationData = destination.mData else { continue }
            let byteCount = min(Int(source.mDataByteSize), Int(destination.mDataByteSize))
            memcpy(destinationData, sourceData, byteCount)
        }

        return buffer
    }

    private func applyGain(to buffer: AVAudioPCMBuffer) {
        guard gain != 1.0 else { return }
        guard buffer.format.commonFormat == .pcmFormatFloat32 else { return }
        guard let channels = buffer.floatChannelData else { return }

        let channelCount = Int(buffer.format.channelCount)
        let frameLength = Int(buffer.frameLength)
        for channel in 0..<channelCount {
            let samples = channels[channel]
            for frame in 0..<frameLength {
                let value = samples[frame] * gain
                samples[frame] = max(-1.0, min(1.0, value))
            }
        }
    }
}
