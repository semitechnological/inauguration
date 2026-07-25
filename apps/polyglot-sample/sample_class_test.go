package main

import (
	"testing"
)

func TestCalculator_Add(t *testing.T) {
	calc := NewCalculator()

	if calc.value != 0 {
		t.Errorf("Expected initial value to be 0, got %d", calc.value)
	}

	result := calc.Add(5)
	if result != 5 {
		t.Errorf("Expected result to be 5, got %d", result)
	}
	if calc.value != 5 {
		t.Errorf("Expected internal value to be 5, got %d", calc.value)
	}

	result = calc.Add(-3)
	if result != 2 {
		t.Errorf("Expected result to be 2, got %d", result)
	}
	if calc.value != 2 {
		t.Errorf("Expected internal value to be 2, got %d", calc.value)
	}
}
