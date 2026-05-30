package main

type Calculator struct {
	value int
}

func NewCalculator() Calculator {
	return Calculator{value: 0}
}

func (c *Calculator) Answer() int {
	return 42
}

func (c *Calculator) Add(x int) int {
	c.value = c.value + x
	return c.value
}

func answer() int {
	calc := NewCalculator()
	return calc.Answer()
}

func main() {}
