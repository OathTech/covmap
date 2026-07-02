# Task Queue Spec

A bounded FIFO queue of tasks, processed oldest-first.
Capacity is fixed at construction.
Overflow is an error, never a silent drop.

## Enqueue

`enqueue(t)` appends task t to the back. It must reject the task with a
QueueFull error when the queue is already at capacity.

## Dequeue

`dequeue()` removes and returns the front task, or None when the queue is empty.

## Size

`size()` returns the number of queued tasks, always within capacity.
