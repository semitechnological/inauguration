import Foundation
import SwiftPreviewHost
import Darwin

@main
enum SwiftPreviewHostClientMain {
    static func main() async {
        let socketPath = CommandLine.arguments.dropFirst().first ?? ".brisk/hotreload/daemon.sock"
        guard let fd = connectUnixSocket(path: socketPath) else {
            fputs("swift-preview-host-client: socket unavailable at \(socketPath)\n", stderr)
            exit(2)
        }

        let host = PreviewHost()
        let handle = FileHandle(fileDescriptor: fd, closeOnDealloc: true)
        let decoder = JSONDecoder()
        var wireDecoder = WireStreamDecoder()
        while true {
            let data = handle.availableData
            if data.isEmpty {
                break
            }
            for event in wireDecoder.ingest(data, decoder: decoder) {
                switch event {
                case .envelope(let envelope):
                    do {
                        let patch = try envelope.toReloadPatch()
                        await host.apply(patch)
                        let restartCount = await host.restartCounter()
                        print("applied patch \(envelope.patchID) target=\(patch.target) reason=\(envelope.reason) restarts=\(restartCount)")
                    } catch PreviewHostDecodeError.unsupportedProtocolVersion(let version) {
                        fputs("swift-preview-host-client: dropped line (unsupported protocol_version=\(version))\n", stderr)
                    } catch {
                        fputs("swift-preview-host-client: dropped line (decode error: \(error))\n", stderr)
                    }
                case .dropped(let reason):
                    if reason == "pending_buffer_overflow" {
                        fputs("swift-preview-host-client: pending buffer overflow, dropping buffered data\n", stderr)
                    } else {
                        fputs("swift-preview-host-client: dropped line (\(reason))\n", stderr)
                    }
                }
            }
        }
        if wireDecoder.droppedLines > 0 {
            fputs("swift-preview-host-client: dropped_lines=\(wireDecoder.droppedLines)\n", stderr)
        }
    }

    private static func connectUnixSocket(path: String) -> Int32? {
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
}
