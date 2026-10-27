// Native regression trigger: publish/remove an aggregate device using the existing output.
// No driver is installed; every device created here is destroyed before this process exits.
import CoreAudio
import Foundation

func checked(_ status: OSStatus, _ action: String) throws {
    if status != noErr { throw NSError(domain: "VesselAudioProbe", code: Int(status), userInfo: [NSLocalizedDescriptionKey: action]) }
}

var address = AudioObjectPropertyAddress(mSelector: kAudioHardwarePropertyDefaultOutputDevice, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
var output = AudioDeviceID(0)
var size = UInt32(MemoryLayout<AudioDeviceID>.size)
try checked(AudioObjectGetPropertyData(AudioObjectID(kAudioObjectSystemObject), &address, 0, nil, &size, &output), "read default output")
var uidPointer: UnsafeRawPointer?
address.mSelector = kAudioDevicePropertyDeviceUID
size = UInt32(MemoryLayout<UnsafeRawPointer>.size)
try checked(AudioObjectGetPropertyData(output, &address, 0, nil, &size, &uidPointer), "read output UID")
let uid = Unmanaged<CFString>.fromOpaque(uidPointer!).takeRetainedValue()

let switchDefault = CommandLine.arguments.contains("--switch-default")
let cycles = CommandLine.arguments.contains("--stress") ? 20 : 3
let owner = getppid()
var completed = 0
for _ in 0..<cycles {
    if getppid() != owner { break }
    var aggregate = AudioDeviceID(0)
    let description: [String: Any] = [
        kAudioAggregateDeviceNameKey: "Vessel temporary audio regression device",
        kAudioAggregateDeviceUIDKey: "app.vessel.test.\(UUID().uuidString)",
        kAudioAggregateDeviceSubDeviceListKey: [[kAudioSubDeviceUIDKey: uid]],
        kAudioAggregateDeviceMainSubDeviceKey: uid,
        kAudioAggregateDeviceIsPrivateKey: false,
    ]
    try checked(AudioHardwareCreateAggregateDevice(description as CFDictionary, &aggregate), "create temporary aggregate")
    do {
        var live = true
        var restoreNeeded = switchDefault
        defer {
            if restoreNeeded {
                address.mSelector = kAudioHardwarePropertyDefaultOutputDevice
                AudioObjectSetPropertyData(AudioObjectID(kAudioObjectSystemObject), &address, 0, nil, UInt32(MemoryLayout<AudioDeviceID>.size), &output)
            }
            if live { AudioHardwareDestroyAggregateDevice(aggregate) }
        }
        if switchDefault {
            address.mSelector = kAudioHardwarePropertyDefaultOutputDevice
            size = UInt32(MemoryLayout<AudioDeviceID>.size)
            try checked(AudioObjectSetPropertyData(AudioObjectID(kAudioObjectSystemObject), &address, 0, nil, size, &aggregate), "select temporary output")
            Thread.sleep(forTimeInterval: 0.2)
        }
        try checked(AudioHardwareDestroyAggregateDevice(aggregate), "remove the active temporary output")
        live = false
        Thread.sleep(forTimeInterval: 0.2)
        if switchDefault {
            address.mSelector = kAudioHardwarePropertyDefaultOutputDevice
            try checked(AudioObjectSetPropertyData(AudioObjectID(kAudioObjectSystemObject), &address, 0, nil, UInt32(MemoryLayout<AudioDeviceID>.size), &output), "restore original output")
            restoreNeeded = false
        }
    }
    Thread.sleep(forTimeInterval: 0.2)
    completed += 1
}
if switchDefault {
    address.mSelector = kAudioHardwarePropertyDefaultOutputDevice
    var restored = AudioDeviceID(0)
    size = UInt32(MemoryLayout<AudioDeviceID>.size)
    try checked(AudioObjectGetPropertyData(AudioObjectID(kAudioObjectSystemObject), &address, 0, nil, &size, &restored), "verify restored output")
    guard restored == output else { throw NSError(domain: "VesselAudioProbe", code: 1, userInfo: [NSLocalizedDescriptionKey: "original output was not restored"]) }
}
print("Native audio device publication/removal completed (\(completed)/\(cycles) cycles); original output verified.")
