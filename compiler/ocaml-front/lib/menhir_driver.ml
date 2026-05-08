open Ast

(** Experimental menhir parser path. Current default parser stays parser.ml until
    grammar coverage reaches feature parity. *)
let parse (_source : string) : program =
  (* Placeholder hook: route to stable parser for now. *)
  Parser.parse _source
