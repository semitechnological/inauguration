// Minimal sources understood by inauguration's in-tree subset front (swift_subset).
// Build without swiftc: IN_NATIVE_SWIFT_SIL=only in build --path apps/native-subset-sample/App.swift --module-id App

struct User

func helper() -> Void {
  return
}

func main(user: User) -> Void {
  helper()
  return
}
