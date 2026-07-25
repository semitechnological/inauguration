package main

import "testing"

func TestCalculator_Answer(t *testing.T) {
	calc := NewCalculator()
	expected := 42
	if got := calc.Answer(); got != expected {
		t.Errorf("Answer() = %v, want %v", got, expected)
	}
}
