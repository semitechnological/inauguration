open Ast

let trim = String.trim

let split_and_trim sep s =
  String.split_on_char sep s |> List.map trim |> List.filter (fun x -> x <> "")

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

let parse_param token =
  match split_and_trim ':' token with
  | [ name; ty ] -> (name, parse_type ty)
  | _ -> (trim token, Named "Unknown")

let parse_func_header line =
  let after_func = String.sub line 5 (String.length line - 5) |> trim in
  let open_idx = String.index_opt after_func '(' in
  let close_idx = String.rindex_opt after_func ')' in
  match (open_idx, close_idx) with
  | Some i, Some j when j > i ->
      let name = String.sub after_func 0 i |> trim in
      let param_blob = String.sub after_func (i + 1) (j - i - 1) in
      let params = if trim param_blob = "" then [] else split_and_trim ',' param_blob |> List.map parse_param in
      let tail =
        if j + 1 < String.length after_func then String.sub after_func (j + 1) (String.length after_func - j - 1)
        else ""
      in
      let ret =
        match String.split_on_char '>' tail with
        | [ _ ] -> Void
        | [ left; right ] when String.ends_with ~suffix:"-" (trim left) -> parse_type right
        | _ -> Void
      in
      { name; params; ret; body = [ Return None ] }
  | _ -> { name = after_func; params = []; ret = Void; body = [ Return None ] }

let parse_struct_line line =
  let raw = String.sub line 7 (String.length line - 7) |> trim in
  let name =
    match String.index_opt raw '{' with
    | Some i -> String.sub raw 0 i |> trim
    | None -> raw
  in
  { name; fields = [] }

let parse source =
  let lines = source |> String.split_on_char '\n' |> List.map trim in
  let rec go acc = function
    | [] -> List.rev acc
    | line :: rest when String.length line = 0 -> go acc rest
    | line :: rest when String.starts_with ~prefix:"func " line ->
        go (Function (parse_func_header line) :: acc) rest
    | line :: rest when String.starts_with ~prefix:"struct " line ->
        go (Struct (parse_struct_line line) :: acc) rest
    | _ :: rest -> go acc rest
  in
  go [] lines
