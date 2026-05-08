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
}
