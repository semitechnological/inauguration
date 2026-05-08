import XCTest
@testable import SwiftPreviewHost

final class PreviewHostTests: XCTestCase {
    func testIncompatiblePatchIncrementsRestartCounter() async {
        let host = PreviewHost()
        await host.apply(.init(target: "App.swift", patchType: .fullModule, compatible: false))
        let count = await host.restartCount
        XCTAssertEqual(count, 1)
    }
}
