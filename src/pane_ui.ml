open Core_kernel

let color_focused = LTerm_style.rgb 255 255 255
let color_unfocused = LTerm_style.rgb 100 100 100

let rec is_ancestor container widget =
  if phys_equal container widget then true
  else
    match widget#parent with
    | Some parent -> is_ancestor container parent
    | None -> false

class t =
  object (self)
    inherit LTerm_widget.t "pane" as super

    val mutable title = Zed_string.of_utf8 ""

    val mutable child : LTerm_widget.t option = None
    method! children = match child with Some widget -> [ widget ] | None -> []

    val mutable focused = false

    initializer
    State.focus
    |> Inc.map ~f:(function
         | Some w -> is_ancestor (self :> LTerm_widget.t) w
         | None -> false)
    |> Inc.observe
    |> Inc.Observer.on_update_exn ~f:(fun upd ->
           let f =
             match upd with
             | Initialized f -> f
             | Changed (_, f) -> f
             | Invalidated -> false
           in
           focused <- f;
           self#queue_draw)

    method! size_request =
      let child_size =
        match child with
        | Some child -> child#size_request
        | None -> { rows = 0; cols = 0 }
      in
      { rows = child_size.rows + 2; cols = child_size.cols + 2 }

    method private compute_allocation =
      match child with
      | Some widget ->
          let rect = self#allocation in
          let row1 = min rect.row2 (rect.row1 + 1)
          and col1 = min rect.col2 rect.col1 in
          widget#set_allocation
            {
              row1;
              col1;
              row2 = max row1 (rect.row2 - 1);
              col2 = max col1 (rect.col2 - 1);
            }
      | None -> ()

    method! set_allocation rect =
      super#set_allocation rect;
      self#compute_allocation

    method set : 'a. (#LTerm_widget.t as 'a) -> unit =
      fun widget ->
        child <- Some (widget :> LTerm_widget.t);
        widget#set_parent (Some (self :> LTerm_widget.t));
        self#queue_draw

    method set_title_utf8 s =
      title <- Zed_string.of_utf8 s;
      self#queue_draw

    method! draw cx focused_w =
      let cx_size = LTerm_draw.size cx in
      let () =
        let style =
          if focused then !Theme.cur.pane_title_focus else !Theme.cur.pane_title
        in
        let buf = Buffer.create cx_size.cols in
        Buffer.add_char buf ' ';
        Buffer.add_string buf (Zed_string.to_utf8 title);
        Buffer.add_string buf
          (String.make (cx_size.cols - Buffer.length buf - 1) ' ');
        let str = Buffer.contents buf in
        LTerm_draw.draw_string cx 0 0 ~style (Zed_string.of_utf8 str)
      in
      LTerm_draw.draw_vline cx 0 (cx_size.cols - 1) cx_size.rows
        ~style:!Theme.cur.split LTerm_draw.Blank;

      match child with
      | Some child ->
          let child_cx =
            LTerm_draw.sub cx
              {
                row1 = 1;
                col1 = 0;
                row2 = cx_size.rows - 1;
                col2 = cx_size.cols - 1;
              }
          in
          child#draw child_cx focused_w
      | None -> ()
  end
