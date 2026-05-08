let () =
  let input = In_channel.input_all In_channel.stdin in
  let program = Ocaml_front.Parser.parse input in
  match Ocaml_front.Checker.check program with
  | Ok () -> print_endline (Ocaml_front.Artifact.program_to_json_lines program)
  | Error (Ocaml_front.Checker.Type_error msg) ->
      prerr_endline msg;
      exit 1
