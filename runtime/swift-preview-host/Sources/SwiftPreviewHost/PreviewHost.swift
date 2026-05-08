import Foundation

public struct ReloadPatch: Codable, Sendable {
    public enum PatchType: String, Codable, Sendable {
        case viewBody
        case modifier
        case fullModule
    }

    public let target: String
    public let patchType: PatchType
    public let compatible: Bool

    public init(target: String, patchType: PatchType, compatible: Bool) {
        self.target = target
        self.patchType = patchType
        self.compatible = compatible
    }
}

public actor PreviewHost {
    private(set) var restartCount: Int = 0

    public init() {}

    public func apply(_ patch: ReloadPatch) async {
        if patch.compatible {
            return
        }
        restartCount += 1
    }

    public func restartCounter() -> Int {
        restartCount
    }
}
