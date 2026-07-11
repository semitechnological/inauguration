sed -i 's/if let Stmt::Try { catches, .. } = body\[0\] {/if let Stmt::Try { catches, .. } = \&body[0] {/g' in-cli/src/compiler/tree_front/extract.rs
cd in-cli && cargo clippy --all-targets --features extended --locked -- -D warnings
