%{
open Ast
%}

%token STRUCT FUNC LET RETURN ARROW COLON COMMA LPAREN RPAREN EOF
%token <string> IDENT
%token <int> INT
%token <string> STRING

%start <Ast.program> program

%%

program:
  | decls EOF { $1 }

 decls:
  |               { [] }
  | d = decl ds = decls { d :: ds }

 decl:
  | STRUCT n = IDENT { Struct { name = n; fields = [] } }
  | FUNC n = IDENT LPAREN params = params RPAREN ret = ret_annot {
      Function { name = n; params; ret; body = [ Return None ] }
    }

 params:
  |                 { [] }
  | p = param       { [p] }
  | p = param COMMA ps = params { p :: ps }

 param:
  | n = IDENT COLON t = IDENT { (n, Named t) }

 ret_annot:
  | ARROW t = IDENT { Named t }
  |                { Void }
