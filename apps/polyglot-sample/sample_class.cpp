class Calculator {
public:
    int value;

    Calculator() : value(0) {}

    int answer() const {
        return 42;
    }

    int add(int x) {
        value = value + x;
        return value;
    }
};

int answer() {
    Calculator calc;
    return calc.answer();
}

int main() {
    return 0;
}
