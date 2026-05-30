typedef struct {
    int value;
} Calculator;

Calculator new_calculator(void) {
    Calculator c = {0};
    return c;
}

int calculator_answer(Calculator* c) {
    return 42;
}

int calculator_add(Calculator* c, int x) {
    c->value = c->value + x;
    return c->value;
}

int answer(void) {
    Calculator calc = new_calculator();
    return calculator_answer(&calc);
}

int main(void) {
    return 0;
}
