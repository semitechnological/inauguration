def answer(): Int = {
  42
}

def main(): Unit = {
}

class Counter(val value: Int) {
  def inc(): Int = {
    value + 1
  }
  def get(): Int = value
}

trait Drawable {
  def draw(): String
}
