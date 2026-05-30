function answer() {
  return 42;
}

function main() {}

class Counter {
  constructor(start) {
    this.value = start;
  }
  inc() {
    this.value = this.value + 1;
    return this.value;
  }
}

const double = function(n) {
  return n * 2;
};

const triple = (n) => {
  return n * 3;
};
