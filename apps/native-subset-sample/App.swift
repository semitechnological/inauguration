// Swift sample for inauguration's Tree-sitter Core IR front.
// Build: in build --path apps/native-subset-sample/App.swift --module-id App

struct User {
  var id: Int
  var name: String
}

func userName(user: User) -> String {
  return user.name
}

func main(user: User) -> String {
  return userName(user)
}
