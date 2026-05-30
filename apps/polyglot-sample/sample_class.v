module main

struct Calculator {
	value int
}

fn new_calculator() Calculator {
	return Calculator{value: 0}
}

fn (c Calculator) answer() int {
	return 42
}

fn (mut c Calculator) add(x int) int {
	c.value = c.value + x
	return c.value
}

fn answer() int {
	calc := new_calculator()
	return calc.answer()
}

fn main() {}
