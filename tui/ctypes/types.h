
#include <stdarg.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>


typedef struct rect {
  uint16_t x;
  uint16_t y;
  uint16_t w;
  uint16_t h;
} rect;


typedef enum constraint_tag {
  constraint_percentage,
  constraint_ratio,
  constraint_length,
  constraint_min,
  constraint_max,
} constraint_tag;

typedef union constraint_body {
  struct {
    uint16_t percentage_0;
  };
  struct {
    uint32_t ratio_0;
    uint32_t ratio_1;
  };
  struct {
    uint16_t length_0;
  };
  struct {
    uint16_t min_0;
  };
  struct {
    uint16_t max_0;
  };
} constraint_body;

typedef struct constraint {
  constraint_tag tag;
  constraint_body body;
} constraint;


typedef enum direction_tag {
  direction_horizontal,
  direction_vertical,
} direction_tag;

typedef union direction_body {
  struct {
    
  };
  struct {
    
  };
} direction_body;

typedef struct direction {
  direction_tag tag;
  direction_body body;
} direction;


typedef enum color_tag {
  color_reset,
  color_black,
  color_red,
  color_green,
  color_yellow,
  color_blue,
  color_magenta,
  color_cyan,
  color_gray,
  color_dark_gray,
  color_light_red,
  color_light_green,
  color_light_yellow,
  color_light_blue,
  color_light_magenta,
  color_light_cyan,
  color_white,
  color_rgb,
  color_indexed,
} color_tag;

typedef union color_body {
  struct {
    
  };
  struct {
    
  };
  struct {
    
  };
  struct {
    
  };
  struct {
    
  };
  struct {
    
  };
  struct {
    
  };
  struct {
    
  };
  struct {
    
  };
  struct {
    
  };
  struct {
    
  };
  struct {
    
  };
  struct {
    
  };
  struct {
    
  };
  struct {
    
  };
  struct {
    
  };
  struct {
    
  };
  struct {
    uint8_t rgb_0;
    uint8_t rgb_1;
    uint8_t rgb_2;
  };
  struct {
    uint8_t indexed_0;
  };
} color_body;

typedef struct color {
  color_tag tag;
  color_body body;
} color;


typedef enum color_opt_tag {
  color_opt_some,
  color_opt_none,
} color_opt_tag;

typedef union color_opt_body {
  struct {
    color some_0;
  };
  struct {
    
  };
} color_opt_body;

typedef struct color_opt {
  color_opt_tag tag;
  color_opt_body body;
} color_opt;


typedef struct style {
  color_opt fg;
  color_opt bg;
  uint16_t add_modifier;
  uint16_t sub_modifier;
} style;


typedef enum key_code_tag {
  key_code_backspace,
  key_code_enter,
  key_code_left,
  key_code_right,
  key_code_up,
  key_code_down,
  key_code_home,
  key_code_end_,
  key_code_page_up,
  key_code_page_down,
  key_code_tab,
  key_code_back_tab,
  key_code_delete,
  key_code_insert,
  key_code_f,
  key_code_char,
  key_code_null,
  key_code_esc,
} key_code_tag;

typedef union key_code_body {
  struct {
    
  };
  struct {
    
  };
  struct {
    
  };
  struct {
    
  };
  struct {
    
  };
  struct {
    
  };
  struct {
    
  };
  struct {
    
  };
  struct {
    
  };
  struct {
    
  };
  struct {
    
  };
  struct {
    
  };
  struct {
    
  };
  struct {
    
  };
  struct {
    uint8_t f_0;
  };
  struct {
    uint32_t char_0;
  };
  struct {
    
  };
  struct {
    
  };
} key_code_body;

typedef struct key_code {
  key_code_tag tag;
  key_code_body body;
} key_code;


typedef struct key_mods {
  uint8_t shift;
  uint8_t control;
  uint8_t alt;
} key_mods;


typedef struct key_event {
  key_code code;
  key_mods modifiers;
} key_event;


typedef enum mouse_button_tag {
  mouse_button_left,
  mouse_button_right,
  mouse_button_middle,
} mouse_button_tag;

typedef union mouse_button_body {
  struct {
    
  };
  struct {
    
  };
  struct {
    
  };
} mouse_button_body;

typedef struct mouse_button {
  mouse_button_tag tag;
  mouse_button_body body;
} mouse_button;


typedef enum mouse_event_kind_tag {
  mouse_event_kind_down,
  mouse_event_kind_up,
  mouse_event_kind_drag,
  mouse_event_kind_moved,
  mouse_event_kind_scroll_down,
  mouse_event_kind_scroll_up,
} mouse_event_kind_tag;

typedef union mouse_event_kind_body {
  struct {
    mouse_button down_0;
  };
  struct {
    mouse_button up_0;
  };
  struct {
    mouse_button drag_0;
  };
  struct {
    
  };
  struct {
    
  };
  struct {
    
  };
} mouse_event_kind_body;

typedef struct mouse_event_kind {
  mouse_event_kind_tag tag;
  mouse_event_kind_body body;
} mouse_event_kind;


typedef struct mouse_event {
  mouse_event_kind kind;
  uint16_t column;
  uint16_t row;
  key_mods modifiers;
} mouse_event;


typedef enum event_tag {
  event_key,
  event_mouse,
  event_resize,
  event_finished,
} event_tag;

typedef union event_body {
  struct {
    key_event key_0;
  };
  struct {
    mouse_event mouse_0;
  };
  struct {
    uint16_t resize_0;
    uint16_t resize_1;
  };
  struct {
    
  };
} event_body;

typedef struct event {
  event_tag tag;
  event_body body;
} event;

