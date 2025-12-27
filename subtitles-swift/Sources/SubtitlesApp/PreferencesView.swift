import SwiftUI

struct PreferencesView: View {
    @AppStorage("subtitles.inputLanguage") private var inputLanguage: String = "auto"
    @AppStorage("subtitles.outputMode") private var outputMode: String = "bilingual"
    @AppStorage("subtitles.outputSampleRate") private var outputSampleRate: String = "16000"
    @AppStorage("subtitles.outputChannels") private var outputChannels: String = "1"
    @AppStorage("subtitles.audioGain") private var audioGain: Double = 1.0
    @AppStorage("subtitles.includeSelfAudio") private var includeSelfAudio: Bool = false
    @AppStorage("subtitles.allowCloudRecognition") private var allowCloudRecognition: Bool = false
    @AppStorage("subtitles.debugOverlay") private var debugOverlay: Bool = false

    var body: some View {
        Form {
            Section("Input") {
                Picker("Input language", selection: $inputLanguage) {
                    Text("Auto").tag("auto")
                    Text("Japanese").tag("japanese")
                    Text("English").tag("english")
                    Text("Chinese").tag("chinese")
                }
                .pickerStyle(.segmented)
                Text("Selecting a language disables translation auto-detect.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Section("Output") {
                Picker("Subtitle output", selection: $outputMode) {
                    Text("Bilingual").tag("bilingual")
                    Text("English only").tag("english")
                    Text("Chinese only").tag("chinese")
                    Text("English + Chinese").tag("english-chinese")
                    Text("Original only").tag("original")
                }

                HStack(spacing: 16) {
                    Picker("Sample rate", selection: $outputSampleRate) {
                        Text("16 kHz").tag("16000")
                        Text("32 kHz").tag("32000")
                        Text("44.1 kHz").tag("44100")
                        Text("48 kHz").tag("48000")
                    }
                    .frame(width: 160)
                    Picker("Channels", selection: $outputChannels) {
                        Text("Mono").tag("1")
                        Text("Stereo").tag("2")
                    }
                    .frame(width: 120)
                }

                HStack {
                    Text("Audio gain")
                    Slider(value: $audioGain, in: 0.5...4.0, step: 0.1)
                    Text(String(format: "%.1fx", audioGain))
                        .frame(width: 50, alignment: .trailing)
                }

                Text("Higher gain can improve recognition for quiet audio.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Section("Advanced") {
                Toggle("Include this app’s audio in capture", isOn: $includeSelfAudio)
                Toggle("Allow cloud recognition when on-device is unavailable", isOn: $allowCloudRecognition)
                Toggle("Debug overlay", isOn: $debugOverlay)
            }

            Section {
                Button("Reset Defaults") {
                    inputLanguage = "auto"
                    outputMode = "bilingual"
                    outputSampleRate = "16000"
                    outputChannels = "1"
                    audioGain = 1.0
                    includeSelfAudio = false
                    allowCloudRecognition = false
                    debugOverlay = false
                }
                Text("Changes apply automatically. If subtitles are running, capture will restart.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
        .padding(16)
        .frame(width: 520)
    }
}
