# Rehashing Hash Map

A HashMap wrapper that shrinks to fit in small steps.

## Why?

Some applications need a high availability and `HashMap.shrink_to_fit` is an
expensive operation.

## How?

Taking a hit in memory. A `RehashingHashMap` has two HashMap structs and when
shrinking it moves the element from one to the other on every write operation
taken.
