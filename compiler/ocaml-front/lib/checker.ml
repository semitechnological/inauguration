open Ast

exception Type_error of string

let has_main_fn (program : program) =
  List.exists
    (function
      | Function fn -> String.equal fn.name "main"
      | Struct _ -> false)
    program

let check (program : program) =
  if has_main_fn program then Ok ()
  else Error (Type_error "missing required function: main")
