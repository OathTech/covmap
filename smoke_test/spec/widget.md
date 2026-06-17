# Widget Spec

A widget is a toy object used to demonstrate covmap. It holds an integer
counter and supports a few operations.

## Counter

The widget has an integer counter, initialized to zero.

### Increment

`increment` increases the counter by one.

### Decrement

`decrement` decreases the counter by one. The counter is allowed to go negative.

### Reset

`reset` sets the counter back to zero, regardless of its current value.

## Identity

Every widget has a unique string identifier assigned at creation time.

## Status

A widget reports a status string: "ok" when the counter is non-negative,
"warn" otherwise.
