module inc_compiler

@[export: "inc_version"]
fn inc_version() int {
	return 5
}

@[export: "inc_hash_fnv1a"]
fn inc_hash_fnv1a(data &u8, len int) u64 {
	mut h := u64(0xcbf29ce484222325)
	for i in 0 .. len {
		h = (h ^ u64(unsafe { data[i] })) * u64(0x100000001b3)
	}
	return h
}

@[export: "inc_add"]
fn inc_add(a int, b int) int {
	return a + b
}
