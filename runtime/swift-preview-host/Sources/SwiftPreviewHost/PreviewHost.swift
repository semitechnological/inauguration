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

public enum PreviewHostDecodeError: Error, Sendable {
    case unsupportedProtocolVersion(UInt8)
}

public struct PatchEnvelope: Codable, Sendable {
    public struct WirePatch: Codable, Sendable {
        public let target: String
        public let patchType: GeneratedWirePatchType
        public let compatible: Bool

        enum CodingKeys: String, CodingKey {
            case target
            case patchType = "patch_type"
            case compatible
        }
    }

    public let protocolVersion: UInt8
    public let patchID: String
    public let timestampMS: UInt64
    public let patch: WirePatch
    public let reason: String

    enum CodingKeys: String, CodingKey {
        case protocolVersion = "protocol_version"
        case patchID = "patch_id"
        case timestampMS = "timestamp_ms"
        case patch
        case reason
    }

    public func toReloadPatch(expectedProtocolVersion: UInt8 = 1) throws -> ReloadPatch {
        guard protocolVersion == expectedProtocolVersion else {
            throw PreviewHostDecodeError.unsupportedProtocolVersion(protocolVersion)
        }
        let patchType: ReloadPatch.PatchType = switch patch.patchType {
        case .viewBody: .viewBody
        case .modifier: .modifier
        case .fullModule: .fullModule
        }
        return ReloadPatch(target: patch.target, patchType: patchType, compatible: patch.compatible)
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
