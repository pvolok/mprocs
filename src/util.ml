let trim len s = if String.length s > len then String.sub s 0 (max 0 len) else s
