let () =
  let src = "struct User\nfunc main(user: User) -> Void" in
  let program = Ocaml_front.Parser.parse src in
  if List.length program <> 2 then failwith "expected two declarations";
  let diagnostics = Ocaml_front.Checker.check program in
  if diagnostics <> [] then failwith "expected checker success";
  let json =
    Ocaml_front.Artifact.program_to_json "App" "App.swift" program diagnostics
  in
  if not (String.contains json '"') then failwith "expected JSON output"
