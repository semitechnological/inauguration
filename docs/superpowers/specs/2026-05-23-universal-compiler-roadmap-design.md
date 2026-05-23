# Universal Compiler Roadmap Design

## Problem

The project goal is to compile many source languages through one compiler and eventually include the needed runtimes. The current repo already has one Core IR/SIL path and several bounded fronts, but the language ambition is broader than the implemented surface. The design must prevent overstating support while making the ambition executable.

## Scope

This slice delivers a truthful roadmap and a machine-readable language support surface. It does not claim full compilation for every language. It adds direct tracking for the user-named gaps OCaml, Odin, and Hare at level 0 so agents and users see the planned route instead of falling through to Swift or an unknown extension.

## Architecture

`parser_registry` remains the source of parser ids and extension routing. A new `language_support` module exposes a stable list of languages, parser ids, extensions, compatibility levels, fronts, runtime boundaries, examples, and next steps. The CLI adds `in languages` for human output and `in languages --json` for agents.

Docs define the north star, compatibility ladder, runtime policy, and phase order. Existing parser docs reference the new CLI so code and docs can be checked together.

## Data Flow

Source path resolution still goes through `resolve_parser_id`. When a language has a parser id but no compatible grammar, `parse_polyglot_file` fails closed with the existing `.icore` hint. `in languages --json` does not parse source; it reports the static support matrix used by docs, tests, and agents.

## Error Handling

Unsupported languages tracked at level 0 do not fall through to Swift. OCaml `.ml` / `.mli`, Odin `.odin`, and Hare `.ha` resolve to Core IR parser ids and return the `.icore` redirect until a front lands. This is safer than accidental Swift SIL emit or silent external compiler use.

## Testing

Unit tests cover extension routing for OCaml, Odin, and Hare; the support matrix includes every requested language; and the CLI parses `in languages --json`. Full validation uses `in test` from a repo-built `in` binary.

## Implementation Notes

No new dependencies are needed. No runtime claim changes from this slice. Runtime status is explicit per language and remains non-bundled until code, examples, and gates prove otherwise.
