import XCTest
@testable import SwiftPreviewHost

final class PreviewHostTests: XCTestCase {
    func testIncompatiblePatchIncrementsRestartCounter() async {
        let host = PreviewHost()
        await host.apply(.init(target: "App.swift", patchType: .fullModule, compatible: false))
        let count = await host.restartCounter()
        XCTAssertEqual(count, 1)
    }

    func testCompatiblePatchKeepsRestartCounterAtZero() async {
        let host = PreviewHost()
        await host.apply(.init(target: "ContentView.swift", patchType: .viewBody, compatible: true))
        let count = await host.restartCounter()
        XCTAssertEqual(count, 0)
    }

    func testEnvelopeDecodesSnakeCasePatchType() throws {
        let payload = """
        {"protocol_version":1,"patch_id":"p1","timestamp_ms":1,"reason":"patch_applied","patch":{"target":"ContentView.swift","patch_type":"view_body","compatible":true}}
        """
        let envelope = try JSONDecoder().decode(PatchEnvelope.self, from: Data(payload.utf8))
        let patch = try envelope.toReloadPatch()
        XCTAssertEqual(patch.patchType, .viewBody)
        XCTAssertTrue(patch.compatible)
    }

    func testEnvelopeRejectsUnsupportedProtocolVersion() throws {
        let payload = """
        {"protocol_version":2,"patch_id":"p1","timestamp_ms":1,"reason":"patch_applied","patch":{"target":"ContentView.swift","patch_type":"view_body","compatible":true}}
        """
        let envelope = try JSONDecoder().decode(PatchEnvelope.self, from: Data(payload.utf8))
        XCTAssertThrowsError(try envelope.toReloadPatch()) { error in
            guard case PreviewHostDecodeError.unsupportedProtocolVersion(let version) = error else {
                return XCTFail("unexpected error: \\(error)")
            }
            XCTAssertEqual(version, 2)
        }
    }

    func testWireStreamDecoderHandlesChunkBoundaries() throws {
        var decoder = WireStreamDecoder()
        let chunk1 = Data(#"{"protocol_version":1,"patch_id":"p1","timestamp_ms":1,"reason":"patch_applied","patch":{"target":"Con"#.utf8)
        let chunk2 = Data(#"tentView.swift","patch_type":"view_body","compatible":true}}"#.utf8)
        let chunk3 = Data("\n".utf8)

        XCTAssertTrue(decoder.ingest(chunk1).isEmpty)
        XCTAssertTrue(decoder.ingest(chunk2).isEmpty)
        let events = decoder.ingest(chunk3)
        XCTAssertEqual(events.count, 1)
        guard case .envelope(let envelope) = events[0] else {
            return XCTFail("expected decoded envelope")
        }
        let patch = try envelope.toReloadPatch()
        XCTAssertEqual(patch.target, "ContentView.swift")
        XCTAssertEqual(decoder.droppedLines, 0)
    }

    func testWireStreamDecoderCountsMalformedLines() {
        var decoder = WireStreamDecoder()
        let payload = Data("not-json\n".utf8)
        let events = decoder.ingest(payload)
        XCTAssertEqual(events.count, 1)
        guard case .dropped(let reason) = events[0] else {
            return XCTFail("expected dropped event")
        }
        XCTAssertEqual(reason, "decode_error")
        XCTAssertEqual(decoder.droppedLines, 1)
    }

    func testWireStreamDecoderDropsOverflowPendingBuffer() {
        var decoder = WireStreamDecoder(maxPendingBytes: 32)
        let payload = Data(String(repeating: "x", count: 64).utf8)
        let events = decoder.ingest(payload)
        XCTAssertEqual(events.count, 1)
        guard case .dropped(let reason) = events[0] else {
            return XCTFail("expected dropped overflow event")
        }
        XCTAssertEqual(reason, "pending_buffer_overflow")
        XCTAssertEqual(decoder.droppedLines, 1)
    }
}
