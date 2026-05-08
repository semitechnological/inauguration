open Ast

let builtin_type = function Int | String | Bool | Void -> true | Named _ -> false

let type_known known = function
  | Named n -> List.mem n known
  | t -> builtin_type t

let collect_struct_names (program : program) =
  program
  |> List.filter_map (function Struct s -> Some s.name | Function _ -> None)

let duplicate_names names =
  let rec go seen dups = function
    | [] -> dups
    | x :: xs ->
        if List.mem x seen then go seen (x :: dups) xs
        else go (x :: seen) dups xs
  in
  go [] [] names

let check (program : program) : diagnostic list =
  let struct_names = collect_struct_names program in
  let fn_names =
    program
    |> List.filter_map (function Function f -> Some f.name | Struct _ -> None)
  in
  let all_top = struct_names @ fn_names in
  let dupes = duplicate_names all_top in

  let missing_main =
    if List.mem "main" fn_names then []
    else [ { code = "E_MAIN"; message = "missing required function: main" } ]
  in

  let duplicate_diags =
    dupes
    |> List.map (fun name -> { code = "E_DUP_TOP"; message = "duplicate top-level declaration: " ^ name })
  in

  let type_diags =
    program
    |> List.concat_map (function
         | Struct s ->
             s.fields
             |> List.filter_map (fun (field, ty) ->
                    if type_known struct_names ty then None
                    else
                      Some
                        {
                          code = "E_UNKNOWN_TYPE";
                          message = "unknown type in struct field " ^ s.name ^ "." ^ field;
                        })
         | Function f ->
             let param_diags =
               f.params
               |> List.filter_map (fun (param, ty) ->
                      if type_known struct_names ty then None
                      else
                        Some
                          {
                            code = "E_UNKNOWN_TYPE";
                            message = "unknown type in function parameter " ^ f.name ^ "." ^ param;
                          })
             in
             let ret_diags =
               if type_known struct_names f.ret then []
               else [ { code = "E_UNKNOWN_TYPE"; message = "unknown return type in function " ^ f.name } ]
             in
             param_diags @ ret_diags)
  in

  missing_main @ duplicate_diags @ type_diags
