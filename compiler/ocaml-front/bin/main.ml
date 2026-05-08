let infer_module_name source_path =
  source_path
  |> Filename.basename
  |> Filename.remove_extension

let () =
  let source_path =
    if Array.length Sys.argv > 1 then Sys.argv.(1) else "stdin.swift"
  in
  let input = In_channel.input_all In_channel.stdin in
  let program = Ocaml_front.Parser.parse input in
  let diagnostics = Ocaml_front.Checker.check program in
  let module_name = infer_module_name source_path in
  let artifact =
    Ocaml_front.Artifact.program_to_json module_name source_path program diagnostics
  in
  print_endline artifact;
  if diagnostics <> [] then exit 1
