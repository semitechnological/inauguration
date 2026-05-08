# Swift Compiler vs in Pipeline Benchmark

Measured with: raw `swiftc -typecheck`, package-context `swift build`, and `in build`.

| Example | swiftc(ms) | swift build(ms) | in(ms) | in/swift-build | swift build ok | in ok |
|---|---:|---:|---:|---:|:---:|:---:|
| `/Users/undivisible/projects/inauguration/../aurorality/examples/counter/Sources/App.swift` | 193.98 | 1018.65 | 299.69 | 0.294 | ✅ | ✅ |
| `/Users/undivisible/projects/inauguration/../aurorality/examples/basic/Sources/App.swift` | 189.98 | 891.57 | 311.61 | 0.350 | ✅ | ✅ |
| `/Users/undivisible/projects/inauguration/../aurorality/examples/hyperchat/Sources/HyperChatRootView.swift` | 10832.74 | 378.72 | 269.40 | 0.711 | ✅ | ✅ |
