include Logs

let () =
  let log_io = Stdio.Out_channel.create ~append:true "tde.log" in

  let formatter =
    Format.make_formatter (Stdlib.output_substring log_io) (fun () ->
        Stdlib.flush log_io)
  in
  let reporter = Logs.format_reporter ~app:formatter ~dst:formatter () in
  Logs.set_reporter reporter;
  Logs.set_level (Some Debug)
