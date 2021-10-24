#include <stdarg.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>

typedef struct RectC {
  uint16_t x;
  uint16_t y;
  uint16_t w;
  uint16_t h;
} RectC;

typedef enum ConstraintC_Tag {
  Percentage,
  Ratio,
  Length,
  Max,
  Min,
} ConstraintC_Tag;

typedef struct Ratio_Body {
  uint32_t _0;
  uint32_t _1;
} Ratio_Body;

typedef union ConstraintC_Body {
  uint16_t percentage;
  Ratio_Body ratio;
  uint16_t length;
  uint16_t max;
  uint16_t min;
} ConstraintC_Body;

typedef struct ConstraintC {
  ConstraintC_Tag tag;
  ConstraintC_Body body;
} ConstraintC;

struct RectC tui_frame_size(void);

void tui_layout2(struct ConstraintC area);
