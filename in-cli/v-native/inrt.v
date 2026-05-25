module inrt

const trap_overflow_code = 1
const trap_div_zero_code = 2
const trap_unreachable_code = 3

pub enum InrtValueKind {
	none
	int_val
	bool_val
	string_val
	array_val
	struct_val
}

@[export: "inrt_value_kind_tag"]
fn inrt_value_kind_tag(kind InrtValueKind) int {
	return int(kind)
}

@[export: "inrt_int_add"]
fn inrt_int_add(a i64, b i64) i64 {
	return a + b
}

@[export: "inrt_int_sub"]
fn inrt_int_sub(a i64, b i64) i64 {
	return a - b
}

@[export: "inrt_int_mul"]
fn inrt_int_mul(a i64, b i64) i64 {
	return a * b
}

@[export: "inrt_bool_not"]
fn inrt_bool_not(a bool) bool {
	return !a
}

@[export: "inrt_string_len"]
fn inrt_string_len(s &u8, len int) int {
	return len
}

@[export: "inrt_string_eq"]
fn inrt_string_eq(a &u8, a_len int, b &u8, b_len int) bool {
	if a_len != b_len {
		return false
	}
	for i in 0 .. a_len {
		if unsafe { a[i] } != unsafe { b[i] } {
			return false
		}
	}
	return true
}

@[export: "inrt_array_len"]
fn inrt_array_len(ptr &u8, elem_size int) int {
	return 0
}

@[export: "inrt_struct_field_offset"]
fn inrt_struct_field_offset(field_index int, field_sizes &int, field_count int) int {
	if field_index < 0 || field_index >= field_count {
		return -1
	}
	mut offset := 0
	for i in 0 .. field_index {
		offset += unsafe { field_sizes[i] }
	}
	return offset
}

@[export: "inrt_trap"]
fn inrt_trap(reason_code int) {
	eprintln('inrt trap: code=${reason_code}')
	exit(reason_code)
}

@[export: "inrt_eval_answer"]
fn inrt_eval_answer(answer_val i64) int {
	return int(answer_val) & 0xff
}
