module S = LTerm_style

type style = S.t

type t = {
  pane_title : style;
  pane_title_focus : style;
  split : style;
  item : style;
  item_focus : style;
  help : style;
}

let style ?bold ?underline ?blink ?reverse ?fg ?bg () =
  {
    LTerm_style.bold;
    underline;
    blink;
    reverse;
    foreground = fg;
    background = bg;
  }

let rgb = LTerm_style.rgb

let default16 =
  {
    pane_title = style ~fg:S.white ~bg:S.magenta ();
    pane_title_focus = style ~bold:true ~fg:S.black ~bg:S.lblue ();
    split = style ~bg:S.black ();
    item = style ~fg:S.lwhite ();
    item_focus = style ~fg:S.black ~bg:S.lyellow ();
    help = style ~fg:S.black ~bg:S.green ();
  }

let default256 =
  {
    pane_title = style ~bold:true ~fg:(rgb 150 150 150) ~bg:(rgb 59 66 81) ();
    pane_title_focus =
      style ~bold:true ~fg:(rgb 35 44 58) ~bg:(rgb 135 192 208) ();
    split = style ~bg:(rgb 80 80 80) ();
    item = style ~fg:(rgb 200 200 200) ();
    item_focus = style ~fg:(rgb 230 230 230) ~bg:(rgb 110 110 110) ();
    help = style ~fg:(rgb 230 230 230) ~bg:(rgb 76 86 106) ();
  }

let cur = ref default256
