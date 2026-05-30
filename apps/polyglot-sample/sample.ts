function answer(): number {
  return 42;
}

function main(): void {}

class TypedCounter {
  value: number;
  constructor(start: number) {
    this.value = start;
  }
  inc(): number {
    this.value = this.value + 1;
    return this.value;
  }
}

const quadruple = (n: number): number => {
  return n * 4;
};
