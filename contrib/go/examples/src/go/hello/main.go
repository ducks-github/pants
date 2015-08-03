package main

import (
  "flag"

  "contrib/go/examples/src/go/libA"
)

func main() {
  n := flag.Int("n", 1, "print message n times")
  for i := 0; i < *n; i++ {
    println("Hello, world!")
  }
  libA.Speak()
}