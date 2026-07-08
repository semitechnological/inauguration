// Minimal freestanding libc string.h for Space
// Scalar-only functions (no pointer arithmetic due to C frontend limitations)

int strlen(const char* s) {
    int len = 0;
    while (s[len] != 0) {
        len++;
    }
    return len;
}

void memcpy_c(char* dst, const char* src, int n) {
    int i = 0;
    while (i < n) {
        dst[i] = src[i];
        i++;
    }
}

void memset_c(char* dst, int c, int n) {
    int i = 0;
    while (i < n) {
        dst[i] = c;
        i++;
    }
}

int abs_c(int x) {
    if (x < 0) { return -x; }
    return x;
}

int min_c(int a, int b) {
    if (a < b) { return a; }
    return b;
}

int max_c(int a, int b) {
    if (a > b) { return a; }
    return b;
}
