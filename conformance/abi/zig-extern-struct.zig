pub const InSliceU8 = extern struct {
    ptr: [*]const u8,
    len: u64,
};

pub export fn person_new(age: u32) Person {
    return Person{ .name = InSliceU8{ .ptr = undefined, .len = 0 }, .age = age };
}

pub const Person = extern struct {
    name: InSliceU8,
    age: u32,
};

pub fn main() void {}