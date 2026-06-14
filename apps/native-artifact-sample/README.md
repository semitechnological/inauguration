# Native artifact sample

This sample exercises the owned native artifact emitters for a const-evaluable `.in` entry function.

Run:

```bash
bash scripts/check-native-artifact-sample.sh
```

Artifacts are written under `target/in/native-artifact-sample/`.

Supported outputs:

- `answer-linux-x86_64`: ELF64 Linux executable for `x86_64-unknown-linux-gnu`
- `Answer.AppDir`: Linux AppDir containing an ELF64 `AppRun`
- `answer.exe`: PE32+ executable for `x86_64-pc-windows-msvc`
- `Answer.app`: macOS app bundle containing an AArch64 Mach-O executable
- `answer-x86_64.o`: ELF64 relocatable object for `x86_64-unknown-linux-gnu`
- `answer-aarch64.o`: ELF64 relocatable object for `aarch64-unknown-linux-gnu`
- `answer-armv7.o`: ELF32 relocatable object for `armv7-unknown-linux-gnueabihf`
- `libanswer.a`: Mach-O static archive for `aarch64-apple-darwin`
- `answer.wasm`: WebAssembly module for `wasm32-unknown-unknown`

`.AppImage` is intentionally not emitted yet. The owned backend fails closed until this repository owns an AppImage runtime and SquashFS writer.
