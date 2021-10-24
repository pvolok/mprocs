open Ctypes

module Stubs (T : Cstubs_structs.TYPE) = struct
  open T

  type rect
  let rect : rect structure typ = structure "RectC"
  let x = field rect "x" uint16_t
  let y = field rect "y" uint16_t
  let w = field rect "w" uint16_t
  let h = field rect "h" uint16_t
  let () = seal rect

  module Constr = struct
    module Tag = struct
      type t =
        [ `Percentage
        | `Ratio
        | `Length
        | `Min
        | `Max
        ]

      let percentage = constant "Percentage" int64_t
      let ratio = constant "Ratio" int64_t
      let length = constant "Length" int64_t
      let min = constant "Min" int64_t
      let max = constant "Max" int64_t

      let t : t typ =
        enum "ConstraintC_Tag"
          [
            (`Percentage, percentage);
            (`Ratio, ratio);
            (`Length, length);
            (`Min, min);
            (`Max, max);
          ]
    end

    module Body = struct
      module Ratio = struct
        type t
        let t : t structure typ = structure "Ratio_Body"
        let _0 = field t "_0" uint32_t
        let _1 = field t "_1" uint32_t
        let () = seal t
      end

      type t
      let t : t union typ = union "ConstraintC_Body"
      let percentage = field t "percentage" uint16_t
      let ratio = field t "ratio" Ratio.t
      let length = field t "length" uint16_t
      let min = field t "min" uint16_t
      let max = field t "max" uint16_t
      let () = seal t
    end

    type t
    let t : t structure typ = structure "ConstraintC"
    let tag = field t "tag" Tag.t
    let body = field t "body" Body.t
    let () = seal t
  end
end
