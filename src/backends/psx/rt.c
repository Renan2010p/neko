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

/* 128-bit integer helpers. On mips1 there is no `__int128`, so Zig's C backend
 * (zig.h) declares these with the `zig_u128`/`zig_i128` struct ABI:
 *   struct { uint64_t lo; uint64_t hi; }  (16-byte aligned, little-endian)
 * We define the same layout and implement the operations with 64/32-bit math. */

typedef struct { unsigned long long lo __attribute__((aligned(16))); unsigned long long hi; } rt_u128;
typedef struct { unsigned long long lo __attribute__((aligned(16))); signed long long hi; } rt_i128;

typedef unsigned long long rt_u64;
typedef unsigned int rt_u32;

static rt_u64 rt_mulhi(rt_u64 a, rt_u64 b) {
    rt_u32 a0 = (rt_u32) a, a1 = (rt_u32) (a >> 32);
    rt_u32 b0 = (rt_u32) b, b1 = (rt_u32) (b >> 32);
    rt_u64 p00 = (rt_u64) a0 * b0, p01 = (rt_u64) a0 * b1;
    rt_u64 p10 = (rt_u64) a1 * b0, p11 = (rt_u64) a1 * b1;
    rt_u64 mid = (p00 >> 32) + (rt_u32) p01 + (rt_u32) p10;
    return p11 + (p01 >> 32) + (p10 >> 32) + (mid >> 32);
}

static rt_u128 rt_shl(rt_u128 v, int s) {
    rt_u128 r;
    if (s == 0) return v;
    if (s >= 64) {
        r.hi = v.lo << (s - 64);
        r.lo = 0;
    } else {
        r.hi = (v.hi << s) | (v.lo >> (64 - s));
        r.lo = v.lo << s;
    }
    return r;
}

static int rt_cmp(rt_u128 a, rt_u128 b) {
    if (a.hi != b.hi) return a.hi < b.hi ? -1 : 1;
    if (a.lo != b.lo) return a.lo < b.lo ? -1 : 1;
    return 0;
}

static rt_u128 rt_sub(rt_u128 a, rt_u128 b) {
    rt_u128 r;
    r.lo = a.lo - b.lo;
    r.hi = a.hi - b.hi - (a.lo < b.lo ? 1 : 0);
    return r;
}

rt_i128 __multi3(rt_i128 a, rt_i128 b) {
    rt_u64 a0 = a.lo, a1 = (rt_u64) a.hi, b0 = b.lo, b1 = (rt_u64) b.hi;
    rt_u64 lo = a0 * b0;
    rt_u64 hi = a0 * b1 + a1 * b0 + rt_mulhi(a0, b0);
    rt_i128 r;
    r.lo = lo;
    r.hi = (signed long long) hi;
    return r;
}

rt_u128 __udivti3(rt_u128 n, rt_u128 d) {
    rt_u128 q = { 0, 0 }, r = { 0, 0 };
    for (int i = 127; i >= 0; i--) {
        rt_u64 bit = (i >= 64) ? ((n.hi >> (i - 64)) & 1) : ((n.lo >> i) & 1);
        r = rt_shl(r, 1);
        r.lo |= bit;
        if (rt_cmp(r, d) >= 0) {
            r = rt_sub(r, d);
            rt_u128 one = { 1, 0 };
            q.lo |= rt_shl(one, i).lo;
            q.hi |= rt_shl(one, i).hi;
        }
    }
    return q;
}

rt_u128 __umodti3(rt_u128 n, rt_u128 d) {
    rt_u128 r = { 0, 0 };
    for (int i = 127; i >= 0; i--) {
        rt_u64 bit = (i >= 64) ? ((n.hi >> (i - 64)) & 1) : ((n.lo >> i) & 1);
        r = rt_shl(r, 1);
        r.lo |= bit;
        if (rt_cmp(r, d) >= 0) r = rt_sub(r, d);
    }
    return r;
}

rt_i128 __divti3(rt_i128 n, rt_i128 d) {
    rt_u128 un = { n.lo, (rt_u64) n.hi };
    rt_u128 ud = { d.lo, (rt_u64) d.hi };
    int neg = (n.hi < 0) != (d.hi < 0);
    if (n.hi < 0) un = rt_sub((rt_u128){ 0, 0 }, un);
    if (d.hi < 0) ud = rt_sub((rt_u128){ 0, 0 }, ud);
    rt_u128 q = __udivti3(un, ud);
    if (neg) q = rt_sub((rt_u128){ 0, 0 }, q);
    rt_i128 r;
    r.lo = q.lo;
    r.hi = (signed long long) q.hi;
    return r;
}

rt_i128 __modti3(rt_i128 n, rt_i128 d) {
    rt_u128 un = { n.lo, (rt_u64) n.hi };
    rt_u128 ud = { d.lo, (rt_u64) d.hi };
    if (n.hi < 0) un = rt_sub((rt_u128){ 0, 0 }, un);
    if (d.hi < 0) ud = rt_sub((rt_u128){ 0, 0 }, ud);
    rt_u128 r = __umodti3(un, ud);
    if (n.hi < 0) r = rt_sub((rt_u128){ 0, 0 }, r);
    rt_i128 out;
    out.lo = r.lo;
    out.hi = (signed long long) r.hi;
    return out;
}
