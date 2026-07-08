// libc in C that .in can call via extern human fn
// All functions use C ABI (System V AMD64)

int c_strlen(const char* s) {
    int len = 0;
    while (s[len] != 0) { len++; }
    return len;
}

int c_strcmp(const char* a, const char* b) {
    int i = 0;
    while (a[i] != 0 && b[i] != 0 && a[i] == b[i]) { i++; }
    unsigned char ca = (unsigned char)a[i];
    unsigned char cb = (unsigned char)b[i];
    return ca - cb;
}

void c_memcpy(char* dst, const char* src, int n) {
    for (int i = 0; i < n; i++) { dst[i] = src[i]; }
}

void c_memset(char* dst, char c, int n) {
    for (int i = 0; i < n; i++) { dst[i] = c; }
}

int c_memcmp(const char* a, const char* b, int n) {
    for (int i = 0; i < n; i++) {
        if (a[i] != b[i]) { return a[i] < b[i] ? -1 : 1; }
    }
    return 0;
}

int c_atoi(const char* s) {
    int neg = 0, val = 0, i = 0;
    if (s[i] == '-') { neg = 1; i++; }
    else if (s[i] == '+') { i++; }
    while (s[i] >= '0' && s[i] <= '9') {
        val = val * 10 + (s[i] - '0');
        i++;
    }
    return neg ? -val : val;
}

int c_abs(int x) { return x < 0 ? -x : x; }

int c_min(int a, int b) { return a < b ? a : b; }

int c_max(int a, int b) { return a > b ? a : b; }

int c_ispow2(int x) { return x > 0 && (x & (x - 1)) == 0; }
