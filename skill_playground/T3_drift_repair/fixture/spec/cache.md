# Cache Spec

A bounded cache mapping integer keys to integer values.

## Get

`get(k)` returns the value bound to key `k`, or None if `k` is not
present.

## Put

`put(k, v)` binds `k` to `v`. If the cache is at capacity, the
least-recently-used entry is dropped.

## Size

`size()` returns the number of entries currently held.
