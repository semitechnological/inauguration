class Calculator {
    var total: Int = 0

    fun add(value: Int): Int {
        total = total + value
        return total
    }

    fun subtract(value: Int): Int {
        total = total - value
        return total
    }
}

fun answer(): Int {
    return 42
}

fun main() {}
