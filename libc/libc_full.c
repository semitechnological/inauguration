// Comprehensive freestanding C libc for Inauguration
// Covers: string.h, stdlib.h, ctype.h, stdio.h, math.h (basic)
// Compiles to ELF with multi-symbol export via: in compile --emit sci

// ===== string.h =====
int strlen(const char* s) {
    int len = 0;
    while (s[len]) len++;
    return len;
}

void* memcpy(void* dst, const void* src, int n) {
    int i = 0;
    char* d = (char*)dst;
    const char* s = (const char*)src;
    while (i < n) { d[i] = s[i]; i++; }
    return dst;
}

void* memset(void* dst, int c, int n) {
    int i = 0;
    char* d = (char*)dst;
    while (i < n) { d[i] = (char)c; i++; }
    return dst;
}

int memcmp(const void* s1, const void* s2, int n) {
    int i = 0;
    const char* a = (const char*)s1;
    const char* b = (const char*)s2;
    while (i < n) {
        if (a[i] != b[i]) return a[i] < b[i] ? -1 : 1;
        i++;
    }
    return 0;
}

void* memmove(void* dst, const void* src, int n) {
    char* d = (char*)dst;
    const char* s = (const char*)src;
    if (d < s) {
        for (int i = 0; i < n; i++) d[i] = s[i];
    } else {
        for (int i = n - 1; i >= 0; i--) d[i] = s[i];
    }
    return dst;
}

char* strcpy(char* dst, const char* src) {
    int i = 0;
    while (src[i]) { dst[i] = src[i]; i++; }
    dst[i] = 0;
    return dst;
}

char* strncpy(char* dst, const char* src, int n) {
    int i = 0;
    while (i < n && src[i]) { dst[i] = src[i]; i++; }
    while (i < n) { dst[i] = 0; i++; }
    return dst;
}

int strcmp(const char* s1, const char* s2) {
    int i = 0;
    while (s1[i] && s2[i] && s1[i] == s2[i]) i++;
    unsigned char a = s1[i];
    unsigned char b = s2[i];
    return a - b;
}

int strncmp(const char* s1, const char* s2, int n) {
    int i = 0;
    while (i < n && s1[i] && s2[i] && s1[i] == s2[i]) i++;
    if (i >= n) return 0;
    unsigned char a = s1[i];
    unsigned char b = s2[i];
    return a - b;
}

char* strcat(char* dst, const char* src) {
    int i = 0, j = 0;
    while (dst[i]) i++;
    while (src[j]) { dst[i + j] = src[j]; j++; }
    dst[i + j] = 0;
    return dst;
}

char* strchr(const char* s, int c) {
    int i = 0;
    while (s[i]) {
        if (s[i] == (char)c) return (char*)(s + i);
        i++;
    }
    if (c == 0) return (char*)(s + i);
    return 0;
}

char* strrchr(const char* s, int c) {
    int i = 0;
    char* last = 0;
    while (s[i]) {
        if (s[i] == (char)c) last = (char*)(s + i);
        i++;
    }
    if (c == 0) return (char*)(s + i);
    return last;
}

char* strstr(const char* haystack, const char* needle) {
    int i = 0;
    if (!needle[0]) return (char*)haystack;
    while (haystack[i]) {
        int j = 0;
        int match = 1;
        while (needle[j]) {
            if (haystack[i + j] != needle[j]) { match = 0; break; }
            j++;
        }
        if (match) return (char*)(haystack + i);
        i++;
    }
    return 0;
}

int strspn(const char* s, const char* accept) {
    int count = 0;
    while (s[count]) {
        int ok = 0;
        for (int i = 0; accept[i]; i++) {
            if (s[count] == accept[i]) { ok = 1; break; }
        }
        if (!ok) break;
        count++;
    }
    return count;
}

int strcspn(const char* s, const char* reject) {
    int count = 0;
    while (s[count]) {
        for (int i = 0; reject[i]; i++) {
            if (s[count] == reject[i]) return count;
        }
        count++;
    }
    return count;
}

// ===== stdlib.h =====
int atoi(const char* s) {
    int i = 0, sign = 1, result = 0;
    while (s[i] == ' ' || s[i] == '\t' || s[i] == '\n') i++;
    if (s[i] == '-') { sign = -1; i++; }
    else if (s[i] == '+') i++;
    while (s[i] >= '0' && s[i] <= '9') {
        result = result * 10 + (s[i] - '0');
        i++;
    }
    return sign * result;
}

long atol(const char* s) { return (long)atoi(s); }

int abs(int x) { return x < 0 ? -x : x; }
long labs(long x) { return x < 0 ? -x : x; }

int rand(void) {
    static int seed = 1;
    seed = seed * 1103515245 + 12345;
    return (seed / 65536) % 32768;
}

void srand(unsigned int seed) {
    // Use volatile to prevent elimination
    volatile int* p = (volatile int*)0x5000;
    *p = (int)seed;
}

int isdigit(int c) { return c >= '0' && c <= '9'; }
int isalpha(int c) { return (c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z'); }
int isalnum(int c) { return isdigit(c) || isalpha(c); }
int isspace(int c) { return c == ' ' || c == '\t' || c == '\n' || c == '\r' || c == '\f' || c == '\v'; }
int isupper(int c) { return c >= 'A' && c <= 'Z'; }
int islower(int c) { return c >= 'a' && c <= 'z'; }
int isprint(int c) { return c >= 32 && c <= 126; }
int isgraph(int c) { return c > 32 && c <= 126; }
int ispunct(int c) { return isprint(c) && !isalnum(c) && !isspace(c); }
int isxdigit(int c) { return isdigit(c) || (c >= 'A' && c <= 'F') || (c >= 'a' && c <= 'f'); }
int toupper(int c) { return islower(c) ? c - 32 : c; }
int tolower(int c) { return isupper(c) ? c + 32 : c; }

// ===== stdio.h (basic) =====
int sprintf(char* buf, const char* fmt, int a, int b, int c, int d) {
    // Minimal: handles "%s %d\\n" patterns
    int pos = 0;
    int args[4] = {a, b, c, d};
    int argi = 0;
    int i = 0;
    while (fmt[i]) {
        if (fmt[i] == '%') {
            i++;
            if (fmt[i] == 's') {
                // Copy string argument
                int si = 0;
                while (((char*)args[argi])[si]) {
                    buf[pos++] = ((char*)args[argi])[si];
                    si++;
                }
                argi++;
            } else if (fmt[i] == 'd') {
                int val = args[argi++];
                if (val < 0) { buf[pos++] = '-'; val = -val; }
                // Convert to digits in reverse
                char digits[12];
                int di = 0;
                if (val == 0) digits[di++] = '0';
                while (val > 0) { digits[di++] = '0' + (val % 10); val /= 10; }
                while (di > 0) buf[pos++] = digits[--di];
            } else if (fmt[i] == 'x' || fmt[i] == 'X') {
                int val = args[argi++];
                char hex[16] = "0123456789abcdef";
                char digits[16];
                int di = 0;
                if (val == 0) digits[di++] = '0';
                while (val > 0) { digits[di++] = hex[val & 0xF]; val >>= 4; }
                while (di > 0) buf[pos++] = digits[--di];
            } else if (fmt[i] == 'c') {
                buf[pos++] = (char)args[argi++];
            } else if (fmt[i] == '%') {
                buf[pos++] = '%';
            }
            i++;
        } else {
            buf[pos++] = fmt[i];
            i++;
        }
    }
    buf[pos] = 0;
    return pos;
}

// ===== math.h (basic) =====
int min(int a, int b) { return a < b ? a : b; }
int max(int a, int b) { return a > b ? a : b; }

int pow_int(int base, int exp) {
    int result = 1;
    for (int i = 0; i < exp; i++) result *= base;
    return result;
}
