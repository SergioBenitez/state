# State

A library for safe and effortless global and thread-local state management.

```rust
extern crate state;

state::set(42u32);
assert_eq!(state::get::<u32>(), 42);
```

See the [documentation](https://sergio.bz/rustdocs/state) for more.

## Usage

Include `state` in your `Cargo.toml` `[dependencies]`:

```toml
[dependencies]
state = "0.0.5"
```

Thread-local state management is not enabled by default. You can enable it
via the "tls" feature:

```toml
[dependencies]
state = { version = "0.0.5", features = ["tls"] }
```

## License

State is licensed under either of the following, at your option:

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT License ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)
