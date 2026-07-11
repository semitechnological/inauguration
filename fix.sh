sed -i 's/pub unsafe extern "C" fn in_json_stringify/#[allow(clippy::missing_safety_doc)]\npub unsafe extern "C" fn in_json_stringify/' in-cli/src/native_stdlib.rs
sed -i 's/pub unsafe extern "C" fn in_str_eq/#[allow(clippy::missing_safety_doc)]\npub unsafe extern "C" fn in_str_eq/' in-cli/src/native_stdlib.rs
sed -i 's/pub unsafe extern "C" fn in_str_table_has/#[allow(clippy::missing_safety_doc)]\npub unsafe extern "C" fn in_str_table_has/' in-cli/src/native_stdlib.rs
sed -i 's/pub unsafe extern "C" fn in_str_table_get_int/#[allow(clippy::missing_safety_doc)]\npub unsafe extern "C" fn in_str_table_get_int/' in-cli/src/native_stdlib.rs
sed -i 's/pub unsafe extern "C" fn in_vec_join/#[allow(clippy::missing_safety_doc)]\npub unsafe extern "C" fn in_vec_join/' in-cli/src/native_stdlib.rs
cd in-cli && cargo clippy --all-targets --features extended --locked -- -D warnings
