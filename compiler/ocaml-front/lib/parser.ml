open Ast

let trim = String.trim

let parse_type s =
  match trim s with
  | "Int" -> Int
  | "String" -> String
  | "Bool" -> Bool
  | "Void" -> Void
  | other -> Named other

let parse_expr s =
  let s = trim s in
  if s = "true" then BoolLit true
  else if s = "false" then BoolLit false
  else
    match int_of_string_opt s with
    | Some n -> IntLit n
    | None ->
        if String.length s >= 2 && s.[0] = '"' && s.[String.length s - 1] = '"' then
          StringLit (String.sub s 1 (String.length s - 2))
        else Ident s

let parse source =
  let lines = source |> String.split_on_char '\n' |> List.map trim in
  let rec go acc = function
    | [] -> List.rev acc
    | line :: rest when String.length line = 0 -> go acc rest
    | line :: rest when String.starts_with ~prefix:"func " line ->
        let name =
          String.sub line 5 (String.length line - 5) |> String.split_on_char '(' |> List.hd
        in
        let fn = { name = trim name; params = []; ret = Void; body = [ Return None ] } in
        go (Function fn :: acc) rest
    | line :: rest when String.starts_with ~prefix:"struct " line ->
        let name = String.sub line 7 (String.length line - 7) |> trim in
        go (Struct { name; fields = [] } :: acc) rest
    | _ :: rest -> go acc rest
  in
  go [] lines
