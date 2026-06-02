-module(sample).

-export([answer/0, main/0]).

answer() ->
    42.

main() ->
    X = answer(),
    ok.
