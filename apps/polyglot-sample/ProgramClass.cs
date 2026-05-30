interface IResettable {
    void Reset();
}

class Accumulator {
    private int total;

    public int Add(int value) {
        total = total + value;
        return total;
    }

    public int Total { get { return total; } }

    public void Reset() {
        total = 0;
    }
}

class ProgramClass {
    static int answer() {
        return 42;
    }

    static void Main(string[] args) {}
}
