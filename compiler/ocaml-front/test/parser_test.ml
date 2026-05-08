let () =
  let src = "struct User\nfunc main()" in
  let program = Ocaml_front.Parser.parse src in
  if List.length program <> 2 then failwith "expected two declarations";
  match Ocaml_front.Checker.check program with
  | Ok () -> ()
  | Error _ -> failwith "expected checker success"
