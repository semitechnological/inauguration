import Foundation
import SwiftPreviewHost
import Darwin

let socketPath = CommandLine.arguments.dropFirst().first ?? ".brisk/hotreload/daemon.sock"
let host = PreviewHost()

struct Envelope: Codable {
    let protocol_version: Int
    let patch_id: String
    let timestamp_ms: UInt64
    let patch: WirePatch

    struct WirePatch: Codable {
        let target: String
        let patch_type: String
        let compatible: Bool
    }
}

func connectUnixSocket(path: String) -> Int32? {
    let fd = socket(AF_UNIX, SOCK_STREAM, 0)
    if fd < 0 {
        return nil
    }
    var addr = sockaddr_un()
    addr.sun_family = sa_family_t(AF_UNIX)
    let pathBytes = path.utf8CString
    let maxCount = MemoryLayout.size(ofValue: addr.sun_path)
    if pathBytes.count >= maxCount {
        close(fd)
        return nil
    }

    withUnsafeMutableBytes(of: &addr.sun_path) { buffer in
        buffer.initializeMemory(as: CChar.self, repeating: 0)
        _ = pathBytes.withUnsafeBufferPointer { src in
            memcpy(buffer.baseAddress, src.baseAddress, src.count)
        }
    }

    let addrLen = socklen_t(MemoryLayout<sa_family_t>.size + pathBytes.count)
    let status = withUnsafePointer(to: &addr) { ptr in
        ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockPtr in
            connect(fd, sockPtr, addrLen)
        }
    }
    if status != 0 {
        close(fd)
        return nil
    }
    return fd
}

guard let fd = connectUnixSocket(path: socketPath) else {
    fputs("swift-preview-host-client: socket unavailable at \(socketPath)\n", stderr)
    exit(2)
}

let handle = FileHandle(fileDescriptor: fd, closeOnDealloc: true)
let decoder = JSONDecoder()
var pending = ""

while true {
    let data = handle.availableData
    if data.isEmpty {
        break
    }
    pending += String(decoding: data, as: UTF8.self)
    let parts = pending.split(separator: "\n", omittingEmptySubsequences: false)
    if parts.isEmpty {
        continue
    }
    let completeCount = pending.hasSuffix("\n") ? parts.count : parts.count - 1
    for index in 0..<max(0, completeCount) {
        let chunk = String(parts[index])
        guard !chunk.isEmpty,
              let line = chunk.data(using: .utf8),
              let env = try? decoder.decode(Envelope.self, from: line) else {
            continue
        }
        let patchType: ReloadPatch.PatchType = switch env.patch.patch_type {
        case "ViewBody": .viewBody
        case "Modifier": .modifier
        default: .fullModule
        }
        let patch = ReloadPatch(
            target: env.patch.target,
            patchType: patchType,
            compatible: env.patch.compatible
        )
        Task {
            await host.apply(patch)
            let restartCount = await host.restartCounter()
            print("applied patch \(env.patch_id) target=\(patch.target) restarts=\(restartCount)")
        }
    }
    pending = pending.hasSuffix("\n") ? "" : String(parts.last ?? "")
}
