type t = {
  x : int;
  y : int;
  w : int;
  h : int;
}

let sub ?(l = 0) ?(t = 0) ?(r = 0) ?(b = 0) rect =
  { x = rect.x + l; y = rect.y + t; w = rect.w - l - r; h = rect.h - t - b }

let to_string r = Printf.sprintf "x: %d, y: %d, w:%d, h:%d" r.x r.y r.w r.h

let iter f rect =
  for y = rect.y to rect.y + rect.h - 1 do
    for x = rect.x to rect.x + rect.w - 1 do
      f x y
    done
  done
