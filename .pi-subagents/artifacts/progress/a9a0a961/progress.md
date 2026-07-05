# Progress a9a0a961

## Status
Scout complete — parser/type extension map + 5 small high-impact improvements.

## Artifact
`/tmp/in-parser-types-scout.md`

## Summary
- Extension points: `parser_registry`, `tree_front/extract`, `in_lang_parse`, `typecheck::{typecheck_resolved,normalize_*}`.
- Main gap: three registries (language_support vs uses_family_typecheck vs try_lang_for) drift.
- Top fix: unify typecheck eligibility; then main alias + type aliases.