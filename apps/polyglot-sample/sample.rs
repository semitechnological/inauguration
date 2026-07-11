fn answer() -> i64 {
    42
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_answer() {
        assert_eq!(answer(), 42);
    }
}
