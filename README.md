# Rehashing Hash Map

[![Build Status](https://travis-ci.org/seppo0010/rehashinghashmap.svg?branch=master)](https://travis-ci.org/seppo0010/rehashinghashmap)

A HashMap wrapper that shrinks to fit in small steps.

## Why?

Some applications need a high availability and `HashMap.shrink_to_fit` is an
expensive operation.

## How?

Taking a hit in memory. A `RehashingHashMap` has two HashMap structs and when
shrinking it moves the element from one to the other on every write operation
taken.

## When?

In situations where you want to claim the memory back after removing elements
from a set, but you cannot take a big downtime.
