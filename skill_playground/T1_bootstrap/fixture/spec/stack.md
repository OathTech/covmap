# Stack Spec

A stack is a last-in, first-out container of integers.

## Push

`push(x)` appends `x` to the top of the stack. The stack has no fixed
capacity.

## Pop

`pop()` removes and returns the top element. It is an error to pop
from an empty stack.

## Peek

`peek()` returns the top element without removing it. It is an error
to peek into an empty stack.

## Length

`len()` returns the number of elements currently on the stack.

## Empty

`is_empty()` returns true iff the stack contains no elements.
