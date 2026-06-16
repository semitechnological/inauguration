struct Pair {
  var left: Int
  var right: Int
}

func left(pair: Pair) -> Int {
  return pair.left
}

func main(pair: Pair) -> Int {
  return left(pair)
}
