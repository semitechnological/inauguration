# Swift Compiler vs in Pipeline Benchmark

Measured with: raw `swiftc -typecheck`, package-context `swift build`, and `in build`.

| Example | swiftc(ms) | swift build(ms) | in(ms) | in/swift-build | swift build ok | in ok |
|---|---:|---:|---:|---:|:---:|:---:|
| `/Users/undivisible/projects/inauguration/../aurorality/examples/counter/Sources/App.swift` | 302.33 | 6680.43 | 332.7 | 0.05 | ✅ | ✅ |
| `/Users/undivisible/projects/inauguration/../aurorality/examples/basic/Sources/App.swift` | 335.08 | 1380.58 | 260.87 | 0.189 | ✅ | ✅ |
| `/Users/undivisible/projects/inauguration/../aurorality/examples/hyperchat/Sources/HyperChatRootView.swift` | 11036.34 | 987.35 | 498.54 | 0.505 | ✅ | ✅ |
