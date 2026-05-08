# Swift Compiler vs in Pipeline Benchmark

Measured with: raw `swiftc -typecheck`, package-context `swift build`, and `in build`.

| Example | swiftc(ms) | swift build(ms) | in(ms) | in/swift-build | swift build ok | in ok |
|---|---:|---:|---:|---:|:---:|:---:|
| `/Users/undivisible/projects/inauguration/../aurorality/examples/counter/Sources/App.swift` | 173.66 | 1201.18 | 439.77 | 0.366 | ✅ | ✅ |
| `/Users/undivisible/projects/inauguration/../aurorality/examples/basic/Sources/App.swift` | 168.65 | 756.74 | 251.36 | 0.332 | ✅ | ✅ |
| `/Users/undivisible/projects/inauguration/../aurorality/examples/hyperchat/Sources/HyperChatRootView.swift` | 9701.12 | 1170.97 | 248.26 | 0.212 | ✅ | ✅ |
