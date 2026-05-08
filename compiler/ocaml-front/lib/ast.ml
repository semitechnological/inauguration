type typ = Int | String | Bool | Void | Named of string

type expr =
  | IntLit of int
  | StringLit of string
  | BoolLit of bool
  | Ident of string

type stmt =
  | Let of string * typ option * expr
  | Return of expr option

type fn_decl = {
  name : string;
  params : (string * typ) list;
  ret : typ;
  body : stmt list;
}

type struct_decl = {
  name : string;
  fields : (string * typ) list;
}

type decl =
  | Struct of struct_decl
  | Function of fn_decl

type program = decl list
