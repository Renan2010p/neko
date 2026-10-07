/* SPDX-License-Identifier: GPL-3.0-or-later
 * Copyright (C) 2026 Renan Lucas Vieira Hilário
 *
 * Minimal FreeType shim for the PS2 backend.
 *
 * Zig cannot translate-c FreeType on this target (the PS2DEV newlib headers
 * need compiler-internal macros), and the FT_FaceRec/FT_GlyphSlotRec layouts
 * are large and version-specific. So this tiny C file is the only thing that
 * knows FreeType: it opens a TTF and hands back one rasterised glyph at a time
 * as an 8-bit coverage bitmap. The Zig backend copies that into a glyph atlas.
 *
 * Built with ee-gcc and linked with -lfreetype -lpng -lz.
 */

#include <ft2build.h>
#include FT_FREETYPE_H

#define NEKO_MAX_FACES 16

static FT_Library g_lib;
static FT_Face g_faces[NEKO_MAX_FACES];
static int g_used[NEKO_MAX_FACES];

typedef struct {
    int width, height, left, top, advance, pitch;
    const unsigned char *coverage;
} neko_glyph_t;

int neko_font_init(void) {
    if (g_lib) return 0;
    return FT_Init_FreeType(&g_lib);
}

int neko_font_open(const char *path, int pixel_size) {
    int i;
    if (neko_font_init() != 0) return -1;
    for (i = 0; i < NEKO_MAX_FACES; i++) {
        if (g_used[i]) continue;
        if (FT_New_Face(g_lib, path, 0, &g_faces[i]) != 0) return -1;
        FT_Set_Pixel_Sizes(g_faces[i], 0, (FT_UInt) pixel_size);
        g_used[i] = 1;
        return i;
    }
    return -1;
}

void neko_font_close(int slot) {
    if (slot < 0 || slot >= NEKO_MAX_FACES || !g_used[slot]) return;
    FT_Done_Face(g_faces[slot]);
    g_used[slot] = 0;
}

/* Distance from the baseline to the highest point (pixels). */
int neko_font_ascender(int slot) {
    if (slot < 0 || slot >= NEKO_MAX_FACES || !g_used[slot]) return 0;
    return (int) (g_faces[slot]->size->metrics.ascender >> 6);
}

int neko_font_descender(int slot) {
    if (slot < 0 || slot >= NEKO_MAX_FACES || !g_used[slot]) return 0;
    return (int) (-g_faces[slot]->size->metrics.descender >> 6);
}

/* Rasterises `codepoint` (8-bit coverage). The buffer stays valid until the
 * next call for the same slot. Returns 0 on success. */
int neko_font_glyph(int slot, unsigned codepoint, neko_glyph_t *out) {
    FT_GlyphSlot g;
    if (slot < 0 || slot >= NEKO_MAX_FACES || !g_used[slot]) return -1;
    if (FT_Load_Char(g_faces[slot], (FT_ULong) codepoint, FT_LOAD_RENDER) != 0) return -1;
    g = g_faces[slot]->glyph;
    out->width = (int) g->bitmap.width;
    out->height = (int) g->bitmap.rows;
    out->left = g->bitmap_left;
    out->top = g->bitmap_top;
    out->advance = (int) (g->advance.x >> 6);
    out->pitch = g->bitmap.pitch;
    out->coverage = g->bitmap.buffer;
    return 0;
}
