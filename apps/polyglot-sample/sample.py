def answer() -> int:
    return 42

def main() -> None:
    pass


class Counter:
    value: int

    def __init__(self, start: int) -> None:
        self.value = start

    def inc(self) -> int:
        value = self.value + 1
        self.value = value
        return value


double = lambda n: n * 2
