/* SPDX-License-Identifier: GPL-3.0-or-later
 * Copyright (C) 2026 Renan Lucas Vieira Hilário
 *
 * Freestanding bits the Zig C backend needs but libgcc does not provide (the
 * mipsel musl toolchain is linked with -nostdlib, so no libc). Soft-float and
 * the rest of compiler-rt come from libgcc.
 */

typedef __SIZE_TYPE__ size_t;

void *memcpy(void *dest, const void *src, size_t n) {
    unsigned char *d = dest;
    const unsigned char *s = src;
    for (size_t i = 0; i < n; i++) d[i] = s[i];
    return dest;
}

void *memset(void *dest, int c, size_t n) {
    unsigned char *d = dest;
    for (size_t i = 0; i < n; i++) d[i] = (unsigned char) c;
    return dest;
}

void *memmove(void *dest, const void *src, size_t n) {
    unsigned char *d = dest;
    const unsigned char *s = src;
    if (d < s) {
        for (size_t i = 0; i < n; i++) d[i] = s[i];
    } else {
        for (size_t i = n; i > 0; i--) d[i - 1] = s[i - 1];
    }
    return dest;
}

int memcmp(const void *a, const void *b, size_t n) {
    const unsigned char *x = a, *y = b;
    for (size_t i = 0; i < n; i++) {
        if (x[i] != y[i]) return (int) x[i] - (int) y[i];
    }
    return 0;
}

/* Minimal libm subset (the toolchain is linked with -nostdlib). Approximate but
 * good enough for game math. */

float fabsf(float x) {
    union { float f; unsigned u; } v;
    v.f = x;
    v.u &= 0x7fffffffu;
    return v.f;
}

float fminf(float a, float b) { return a < b ? a : b; }
float fmaxf(float a, float b) { return a > b ? a : b; }

float floorf(float x) {
    float t = (float) (int) x;
    return (t > x) ? t - 1.0f : t;
}

float sqrtf(float x) {
    if (x <= 0.0f) return 0.0f;
    union { float f; unsigned u; } v;
    v.f = x;
    v.u = (v.u >> 1) + 0x1fc00000u; /* rough initial guess */
    float r = v.f;
    for (int i = 0; i < 6; i++) r = 0.5f * (r + x / r);
    return r;
}

static float sin_poly(float x) {
    float x2 = x * x;
    return x * (1.0f + x2 * (-1.6666667e-1f + x2 * (8.333333e-3f + x2 * (-1.984127e-4f))));
}

float sinf(float x) {
    const float PI = 3.14159265f, TWO_PI = 6.28318531f, HALF_PI = 1.57079633f;
    int k = (int) (x / TWO_PI);
    x -= (float) k * TWO_PI;
    if (x > PI) x -= TWO_PI;
    else if (x < -PI) x += TWO_PI;
    if (x > HALF_PI) x = PI - x;
    else if (x < -HALF_PI) x = -PI - x;
    return sin_poly(x);
}

float cosf(float x) { return sinf(x + 1.57079633f); }
