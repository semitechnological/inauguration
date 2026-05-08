import Foundation

public enum WireDecodeEvent: Sendable {
    case envelope(PatchEnvelope)
    case dropped(String)
}

public struct WireStreamDecoder: Sendable {
    private(set) var pending: String = ""
    public private(set) var droppedLines: Int = 0
    public let maxPendingBytes: Int

    public init(maxPendingBytes: Int = 1_048_576) {
        self.maxPendingBytes = maxPendingBytes
    }

    public mutating func ingest(_ data: Data, decoder: JSONDecoder = JSONDecoder()) -> [WireDecodeEvent] {
        var events: [WireDecodeEvent] = []
        pending += String(decoding: data, as: UTF8.self)
        if pending.utf8.count > maxPendingBytes {
            droppedLines += 1
            pending = ""
            events.append(.dropped("pending_buffer_overflow"))
            return events
        }

        let parts = pending.split(separator: "\n", omittingEmptySubsequences: false)
        if parts.isEmpty {
            return events
        }

        let completeCount = pending.hasSuffix("\n") ? parts.count : parts.count - 1
        for index in 0..<max(0, completeCount) {
            let chunk = String(parts[index])
            guard !chunk.isEmpty else {
                continue
            }
            guard let line = chunk.data(using: .utf8) else {
                droppedLines += 1
                events.append(.dropped("non_utf8"))
                continue
            }
            do {
                let envelope = try decoder.decode(PatchEnvelope.self, from: line)
                events.append(.envelope(envelope))
            } catch {
                droppedLines += 1
                events.append(.dropped("decode_error"))
            }
        }

        pending = pending.hasSuffix("\n") ? "" : String(parts.last ?? "")
        return events
    }
}
