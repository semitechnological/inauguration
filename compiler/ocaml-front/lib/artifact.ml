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

let program_to_json_lines (program : program) =
  String.concat "\n" (List.map decl_to_json program)
