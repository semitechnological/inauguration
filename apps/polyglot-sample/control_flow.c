int helper(int value) {
    return value;
}

int main(void) {
    int value = 1;
    value = value + 2;
    helper(value);
    if (value > 2) {
        value = value - 1;
    } else {
        value = 0;
    }
    while (value < 4) {
        value = value + 1;
    }
    return value;
}
