// demo.c — C libc functions for multi-language demo
int strlen(const char* s) {
    int len = 0;
    while (s[len]) len++;
    return len;
}

int add(int a, int b) {
    return a + b;
}
