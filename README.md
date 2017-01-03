# State

A library for safe and effortless global and thread-local state management.

```rust
extern crate state;

state::set(42u32);
assert_eq!(state::get::<u32>(), 42);
```
