package main

import (
	"fmt"

	"github.com/gofiber/fiber/v2"
)

func main() {
	_ = fiber.New()
	fmt.Print("fiber")
}