print("Building mprocs.")
vt.start("cargo build"):wait()
print("Built.")

local BIN = "cargo r --"

function test(name, f)
  print("TEST: " .. name)
  f()
end

test("next proc", function()
  local proc = vt.start(BIN .. ' "echo one" "echo two" "echo three"')

  local mark = "•"

  proc:wait_text(mark .. "echo one")

  proc:send_str("j")

  proc:wait_text(mark .. "echo two")

  proc:send_str("q")
  proc:wait()
end)

test("next proc 2", function()
  local proc = vt.start(BIN .. ' "nvim --clean"')

  proc:wait_text("[No Name]")

  proc:send_key("<C-a>")
  proc:send_str("ihello!")
  proc:send_key("<Esc>")
  proc:send_str("i ")

  proc:wait_text("hello !")

  proc:send_key("<C-a>")
  proc:send_str("qy")
  proc:wait()
end)

test("select by mouse", function()
  local proc = vt.start(BIN .. ' "echo one" "echo two" "echo three"')

  local mark = "•"

  proc:wait_text(" echo three")

  proc:click({ x = 1, y = 3 })

  proc:wait_text(mark .. "echo three")

  proc:send_str("q")
  proc:wait()
end)
