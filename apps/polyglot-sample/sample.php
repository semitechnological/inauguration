<?php

function answer(): int {
    return 42;
}

function main(): void {
}

class Counter {
    private int $value = 0;

    function __construct(int $start) {
        $this->value = $start;
    }

    function inc(): int {
        $v = $this->value + 1;
        $this->value = $v;
        return $v;
    }
}

interface Drawable {
    function draw(): string;
}
