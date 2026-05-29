// Minimal sources understood by inauguration's in-tree subset front (swift_subset).
// Build without swiftc: IN_NATIVE_SWIFT_SIL=only in build --path apps/native-subset-sample/App.swift --module-id App

struct User {
  id: Int
  name: String
}

func userName(user: User) -> String {
  return user.name
}

func main(user: User) -> String {
  return userName(user)
}
