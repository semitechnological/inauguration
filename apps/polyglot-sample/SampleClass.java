interface Printable {
  String format();
}

class Counter {
  private int count;

  Counter() {
    count = 0;
  }

  int increment() {
    count = count + 1;
    return count;
  }

  int getValue() {
    return count;
  }
}

class SampleClass {
  public static int answer() {
    return 42;
  }

  public static void main(String[] args) {}
}
