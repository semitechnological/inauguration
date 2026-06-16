struct Account {
  var id: Int
  var owner: String
}

func owner(account: Account) -> String {
  return account.owner
}

func main(account: Account) -> String {
  return owner(account)
}
