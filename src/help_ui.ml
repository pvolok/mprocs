open Core_kernel

class t =
  object (self)
    inherit LTerm_widget.t "help"

    val mutable keymap : (string * LTerm_key.t) list = []

    initializer
    State.keymap |> Inc.observe
    |> Inc.Observer.on_update_exn ~f:(fun upd ->
           (match upd with
           | Initialized k | Changed (_, k) -> keymap <- k
           | Invalidated -> ());
           self#queue_draw)

    method! size_request = { rows = 1; cols = 120 }

    method! draw cx _ =
      let cx_size = LTerm_draw.size cx in

      LTerm_draw.fill_style cx !Theme.cur.help;

      let rec render_next col keys =
        match keys with
        | [] -> ()
        | (desc, key) :: rest ->
            let str =
              "<" ^ LTerm_key.to_string_compact key ^ ": " ^ desc ^ ">  "
            in
            let str =
              if String.length str > cx_size.cols then
                String.sub str ~pos:0 ~len:cx_size.cols
              else str
            in
            let text = LTerm_text.of_utf8 str in
            LTerm_draw.draw_styled cx 0 col ~style:!Theme.cur.help text;
            render_next (col + String.length str) rest
      in
      render_next 1 keymap
  end
