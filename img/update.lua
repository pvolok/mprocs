local proc = vt.start(
  "cargo r -- -c img/mprocs.yaml",
  { width = 90, height = 30 }
)

proc:wait_text("[No Name]")

proc:dump_png("img/screenshot1.png")

proc:send_str("j")
proc:send_key("<C-a>")
proc:wait_text("Listening")

proc:dump_png("img/screenshot2.png")

proc:send_key("<C-a>")
proc:send_str("q")
proc:wait()
