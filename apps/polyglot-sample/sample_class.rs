struct Calculator {
    value: i64,
}

impl Calculator {
    fn new() -> Self {
        Calculator { value: 0 }
    }

    fn answer(&self) -> i64 {
        42
    }

    fn add(&mut self, x: i64) -> i64 {
        self.value = self.value + x;
        self.value
    }
}

fn answer() -> i64 {
    let calc = Calculator::new();
    calc.answer()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculator_add() {
        let mut calc = Calculator::new();

        let result = calc.add(10);
        assert_eq!(result, 10);
        assert_eq!(calc.value, 10);

        let result = calc.add(-5);
        assert_eq!(result, 5);
        assert_eq!(calc.value, 5);

        let result = calc.add(0);
        assert_eq!(result, 5);
        assert_eq!(calc.value, 5);
    }

    #[test]
    fn test_calculator_answer() {
        let calc = Calculator::new();
        assert_eq!(calc.answer(), 42);
    }
}
