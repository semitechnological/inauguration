struct Account {
  id: Int
  owner: String
}

func owner(account: Account) -> String {
  return account.owner
}

func main(account: Account) -> String {
  return owner(account)
}
