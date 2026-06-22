# Ring Buffer Spec

A ring buffer is a fixed-capacity FIFO over bytes.

## Capacity

The capacity is fixed at construction. It must be a power of two so that the implementation can mask indices instead of taking a modulus.

## Push

`push(b)` writes one byte and advances the head.
