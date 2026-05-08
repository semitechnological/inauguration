{
open Parser_menhir

exception Lex_error of string
}

rule token = parse
  | [' ' '\t' '\r' '\n'] { token lexbuf }
  | "struct" { STRUCT }
  | "func" { FUNC }
  | "let" { LET }
  | "return" { RETURN }
  | "->" { ARROW }
  | ":" { COLON }
  | "," { COMMA }
  | "(" { LPAREN }
  | ")" { RPAREN }
  | ['0'-'9']+ as n { INT (int_of_string n) }
  | '"' [^ '"']* '"' as s { STRING (String.sub s 1 (String.length s - 2)) }
  | ['A'-'Z' 'a'-'z' '_']['A'-'Z' 'a'-'z' '0'-'9' '_']* as id { IDENT id }
  | eof { EOF }
  | _ as c { raise (Lex_error (Printf.sprintf "unexpected char: %c" c)) }
