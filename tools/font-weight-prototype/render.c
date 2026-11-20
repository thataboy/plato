// SPDX-License-Identifier: AGPL-3.0-or-later
// Standalone experiment: same unhinted FreeType rasterization as Plato.
#include <ft2build.h>
#include FT_FREETYPE_H
#include FT_OUTLINE_H
#include <hb-ft.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>

#define W 1072
#define H 1920
static unsigned char pixels[W * H];

static void check(int error, const char *operation) {
    if (error) { fprintf(stderr, "%s failed: %d\n", operation, error); exit(1); }
}

static void text(FT_Face face, const char *str, int x, int baseline, double strength) {
    hb_font_t *font = hb_ft_font_create_referenced(face);
    hb_ft_font_set_load_flags(font, FT_LOAD_NO_HINTING);
    hb_buffer_t *buf = hb_buffer_create();
    hb_buffer_add_utf8(buf, str, -1, 0, -1);
    hb_buffer_guess_segment_properties(buf);
    hb_shape(font, buf, NULL, 0);
    unsigned count;
    hb_glyph_info_t *info = hb_buffer_get_glyph_infos(buf, &count);
    hb_glyph_position_t *pos = hb_buffer_get_glyph_positions(buf, &count);
    FT_Pos pen = x * 64;
    for (unsigned i = 0; i < count; ++i) {
        check(FT_Load_Glyph(face, info[i].codepoint, FT_LOAD_NO_HINTING | FT_LOAD_NO_BITMAP), "load glyph");
        FT_GlyphSlot slot = face->glyph;
        if (strength != 0 && slot->format == FT_GLYPH_FORMAT_OUTLINE) {
            FT_Pos amount = lround(strength * 64);
            check(FT_Outline_EmboldenXY(&slot->outline, amount, amount), "adjust outline");
            // Keep expansion/contraction centered around the original outline.
            FT_Outline_Translate(&slot->outline, -amount / 2, -amount / 2);
        }
        check(FT_Render_Glyph(slot, FT_RENDER_MODE_NORMAL), "render glyph");
        int left = (pen + pos[i].x_offset + 32) / 64 + slot->bitmap_left;
        int top = baseline - pos[i].y_offset / 64 - slot->bitmap_top;
        for (unsigned y = 0; y < slot->bitmap.rows; ++y) {
            for (unsigned xx = 0; xx < slot->bitmap.width; ++xx) {
                int px = left + xx, py = top + y;
                if (px < 0 || px >= W || py < 0 || py >= H) continue;
                unsigned alpha = slot->bitmap.buffer[y * slot->bitmap.pitch + xx];
                pixels[py * W + px] = (pixels[py * W + px] * (255 - alpha) + 127) / 255;
            }
        }
        pen += pos[i].x_advance; // Weight never changes shaping or advances.
    }
    hb_buffer_destroy(buf);
    hb_font_destroy(font);
}

int main(int argc, char **argv) {
    if (argc != 5) {
        fprintf(stderr, "Usage: %s font label-font title output.pgm\n", argv[0]);
        return 1;
    }
    FT_Library lib;
    FT_Face face, label;
    check(FT_Init_FreeType(&lib), "init");
    check(FT_New_Face(lib, argv[1], 0, &face), "open sample font");
    check(FT_New_Face(lib, argv[2], 0, &label), "open label font");
    check(FT_Set_Char_Size(face, 11 * 64, 0, 300, 0), "sample size");
    memset(pixels, 255, sizeof(pixels));
    check(FT_Set_Pixel_Sizes(label, 0, 36), "title size");
    text(label, argv[3], 40, 52, 0);
    check(FT_Set_Pixel_Sizes(label, 0, 22), "label size");
    text(label, "11 pt / 300 DPI | unhinted grayscale | fixed glyph advances", 40, 88, 0);
    text(label, "Signed FreeType strength in pixels; X = Y; centered adjustment", 40, 119, 0);
    double strengths[] = {-3, -2, -1.5, -1, -.5, -.25, 0, .25, .5, 1, 1.5, 2, 3};
    for (unsigned i = 0; i < sizeof(strengths) / sizeof(*strengths); ++i) {
        int y = 150 + i * 133;
        for (int x = 40; x < W - 40; ++x) pixels[y * W + x] = 210;
        char caption[100];
        snprintf(caption, sizeof(caption), "%+.2f px%s", strengths[i], strengths[i] == 0 ? "   ORIGINAL" : "");
        text(label, caption, 40, y + 28, 0);
        text(face, "The quick brown fox jumps over.", 40, y + 76, strengths[i]);
        text(face, "Hamburgefontsiv 0123456789 fi fl", 40, y + 123, strengths[i]);
    }
    FILE *out = fopen(argv[4], "wb");
    if (!out) { perror(argv[4]); return 1; }
    fprintf(out, "P5\n%d %d\n255\n", W, H);
    if (fwrite(pixels, 1, sizeof(pixels), out) != sizeof(pixels)) return 1;
    fclose(out);
    FT_Done_Face(face); FT_Done_Face(label); FT_Done_FreeType(lib);
    return 0;
}
