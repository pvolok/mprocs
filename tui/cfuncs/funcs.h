#include "types.h"

void tui_terminal_create();
void tui_terminal_destroy();
void tui_enable_raw_mode();
void tui_disable_raw_mode();
void tui_clear();
void tui_enter_alternate_screen();
void tui_leave_alternate_screen();

rect tui_frame_size();

void tui_layout(constraint *spec, size_t len, direction dir, rect area,
                rect *result);

void tui_render_start();
void tui_render_end();

void tui_render_block(const style *style_opt, const char *title, rect area);

void tui_render_string(const style *style_opt, const char *s, rect area);

event tui_events_read();

extern const uint16_t tui_mod_bold;
extern const uint16_t tui_mod_dim;
extern const uint16_t tui_mod_italic;
extern const uint16_t tui_mod_underlined;
extern const uint16_t tui_mod_slow_blink;
extern const uint16_t tui_mod_rapid_blink;
extern const uint16_t tui_mod_reversed;
extern const uint16_t tui_mod_hidden;
extern const uint16_t tui_mod_crossed_out;
extern const uint16_t tui_mod_empty;
