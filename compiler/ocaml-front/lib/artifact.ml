open Ast

let string_of_type = function
  | Int -> "Int"
  | String -> "String"
  | Bool -> "Bool"
  | Void -> "Void"
  | Named name -> name

let decl_to_json = function
  | Struct s ->
      Printf.sprintf
        {|{"kind":"struct","name":"%s","field_count":%d}|}
        s.name
        (List.length s.fields)
  | Function f ->
      Printf.sprintf
        {|{"kind":"function","name":"%s","ret":"%s","stmt_count":%d}|}
        f.name
        (string_of_type f.ret)
        (List.length f.body)

let diagnostics_to_json diags =
  let chunks =
    diags
    |> List.map (fun d ->
           Printf.sprintf {|{"code":"%s","message":"%s"}|} d.code d.message)
  in
  "[" ^ String.concat "," chunks ^ "]"

let program_to_json
    (module_name : string)
    (source_path : string)
    (program : program)
    (diagnostics : diagnostic list) =
  let structs =
    program
    |> List.filter_map (function
         | Struct s -> Some (Printf.sprintf {|{"name":"%s"}|} s.name)
         | _ -> None)
  in
  let funcs =
    program
    |> List.filter_map (function
         | Function f -> Some (Printf.sprintf {|{"name":"%s"}|} f.name)
         | _ -> None)
  in
  let typed_decls =
    "[" ^ String.concat "," (List.map decl_to_json program) ^ "]"
  in
  Printf.sprintf
    {|{"format_version":1,"module":"%s","source_path":"%s","symbols":{"structs":[%s],"functions":[%s]},"typed_decls":%s,"diagnostics":%s,"success":%s}|}
    module_name
    source_path
    (String.concat "," structs)
    (String.concat "," funcs)
    typed_decls
    (diagnostics_to_json diagnostics)
    (if diagnostics = [] then "true" else "false")
