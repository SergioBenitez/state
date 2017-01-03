# State

A library to effortlessly and safely handle global state.

```rust
extern crate state;

state::put::<u32>(1);
assert_eq!(state::get::<u32>(), 1);
```
