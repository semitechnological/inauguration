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
    fn test_calculator_new() {
        let calc = Calculator::new();
        assert_eq!(calc.value, 0);
    }

    #[test]
    fn test_calculator_answer() {
        let calc = Calculator::new();
        assert_eq!(calc.answer(), 42);
    }

    #[test]
    fn test_calculator_add() {
        let mut calc = Calculator::new();
        assert_eq!(calc.add(10), 10);
        assert_eq!(calc.value, 10);

        assert_eq!(calc.add(-5), 5);
        assert_eq!(calc.value, 5);
    }

    #[test]
    fn test_global_answer() {
        assert_eq!(answer(), 42);
    }
}
