open Core_kernel

let key ?(control = false) ?(meta = false) ?(shift = false) c =
  {
    LTerm_key.control;
    meta;
    shift;
    code = Char (CamomileLibrary.UChar.of_char c);
  }

let procs_help =
  [
    ("Quit", key 'q');
    ("Output", key ~control:true 'a');
    ("Kill", key 'x');
    ("Start", key 's');
    ("Up", key 'k');
    ("Down", key 'j');
  ]

let output_help = [ ("Process list", key ~control:true 'a') ]
