#![doc(html_root_url = "https://docs.rs/state/0.6.0")]
#![warn(missing_docs)]

//! # Safe, Effortless `state` Management
//!
//! This crate allows you to safely and effortlessly manage global and/or
//! thread-local state. Three primitives are provided for state management:
//!
//!  * **[`struct@TypeMap`]:** Type-based storage for many values.
//!  * **[`InitCell`]:** Thread-safe init-once storage for a single value.
//!  * **[`LocalInitCell`]:** Thread-local init-once-per-thread cell.
//!
//! ## Usage
//!
//! Include `state` in your `Cargo.toml` `[dependencies]`:
//!
//! ```toml
//! [dependencies]
//! state = "0.6.0"
//! ```
//!
//! Thread-local state management is not enabled by default. You can enable it
//! via the `tls` feature:
//!
//! ```toml
//! [dependencies]
//! state = { version = "0.6.0", features = ["tls"] }
//! ```
//!
//! ## Use Cases
//!
//! ### Memoizing Expensive Operations
//!
//! The [`InitCell`] type can be used to conveniently memoize expensive
//! read-based operations without needing to mutably borrow. Consider a `struct`
//! with a field `value` and method `compute()` that performs an expensive
//! operation on `value` to produce a derived value. We can use `InitCell` to
//! memoize `compute()`:
//!
//! ```rust
//! use state::InitCell;
//!
//! struct Value;
//! struct DerivedValue;
//!
//! struct Foo {
//!     value: Value,
//!     cached: InitCell<DerivedValue>
//! }
//!
//! impl Foo {
//!     fn set_value(&mut self, v: Value) {
//!         self.value = v;
//!         self.cached.reset();
//!     }
//!
//!     fn compute(&self) -> &DerivedValue {
//!         self.cached.get_or_init(|| {
//!             let _value = &self.value;
//!             unimplemented!("expensive computation with `self.value`")
//!         })
//!     }
//! }
//! ```
//!
//! ### Read-Only Singleton
//!
//! Suppose you have the following structure which is initialized in `main`
//! after receiving input from the user:
//!
//! ```rust
//! struct Configuration {
//!     name: String,
//!     number: isize,
//!     verbose: bool
//! }
//!
//! fn main() {
//!     let config = Configuration {
//!         /* fill in structure at run-time from user input */
//! #        name: "Sergio".to_string(),
//! #        number: 1,
//! #        verbose: true
//!     };
//! }
//! ```
//!
//! You'd like to access this structure later, at any point in the program,
//! without any synchronization overhead. Prior to `state`, assuming you needed
//! to setup the structure after program start, your options were:
//!
//!   1. Use `static mut` and `unsafe` to set an `Option<Configuration>` to
//!      `Some`. Retrieve by checking for `Some`.
//!   2. Use `lazy_static` with a `RwLock` to set an
//!      `RwLock<Option<Configuration>>` to `Some`. Retrieve by `lock`ing and
//!      checking for `Some`, paying the cost of synchronization.
//!
//! With `state`, you can use [`LocalInitCell`] as follows:
//!
//! ```rust
//! # extern crate state;
//! # #[cfg(feature = "tls")]
//! # fn main() {
//! # use state::LocalInitCell;
//! # struct Configuration { name: String, number: isize, verbose: bool }
//! static CONFIG: LocalInitCell<Configuration> = LocalInitCell::new();
//!
//! fn main() {
//!     CONFIG.set(|| Configuration {
//!         /* fill in structure at run-time from user input */
//! #        name: "Sergio".to_string(),
//! #        number: 1,
//! #        verbose: true
//!     });
//!
//!     /* at any point later in the program, in any thread */
//!     let config = CONFIG.get();
//! }
//! # }
//! # #[cfg(not(feature = "tls"))]
//! # fn main() {  }
//! ```
//!
//! Note that you can _also_ use [`InitCell`] to the same effect.
//!
//! ### Read/Write Singleton
//!
//! Following from the previous example, let's now say that we want to be able
//! to modify our singleton `Configuration` structure as the program evolves. We
//! have two options:
//!
//!   1. If we want to maintain the _same_ state in any thread, we can use a
//!      `InitCell` structure and wrap our `Configuration` structure in a
//!      synchronization primitive.
//!   2. If we want to maintain _different_ state in any thread, we can continue
//!      to use a `LocalInitCell` structure and wrap our `LocalInitCell` type in a
//!      `Cell` structure for internal mutability.
//!
//! In this example, we'll choose **1**. The next example illustrates an
//! instance of **2**.
//!
//! The following implements **1** by using a `InitCell` structure and wrapping
//! the `Configuration` type with a `RwLock`:
//!
//! ```rust
//! # struct Configuration { name: String, number: isize, verbose: bool }
//! # use state::InitCell;
//! # use std::sync::RwLock;
//! static CONFIG: InitCell<RwLock<Configuration>> = InitCell::new();
//!
//! fn main() {
//!     let config = Configuration {
//!         /* fill in structure at run-time from user input */
//! #        name: "Sergio".to_string(),
//! #        number: 1,
//! #        verbose: true
//!     };
//!
//!     // Make the config avaiable globally.
//!     CONFIG.set(RwLock::new(config));
//!
//!     /* at any point later in the program, in any thread */
//!     let mut_config = CONFIG.get().write();
//! }
//! ```
//!
//! ### Mutable, thread-local data
//!
//! Imagine you want to count the number of invocations to a function per
//! thread. You'd like to store the count in a `Cell<usize>` and use
//! `count.set(count.get() + 1)` to increment the count. Prior to `state`, your
//! only option was to use the `thread_local!` macro. `state` provides a more
//! flexible, and arguably simpler solution via `LocalInitCell`. This scanario
//! is implemented in the folloiwng:
//!
//! ```rust
//! # extern crate state;
//! # use std::cell::Cell;
//! # use std::thread;
//! # #[cfg(feature = "tls")]
//! # use state::LocalInitCell;
//! # #[cfg(feature = "tls")]
//! static COUNT: LocalInitCell<Cell<usize>> = LocalInitCell::new();
//!
//! # #[cfg(not(feature = "tls"))] fn function_to_measure() { }
//! # #[cfg(feature = "tls")]
//! fn function_to_measure() {
//!     let count = COUNT.get();
//!     count.set(count.get() + 1);
//! }
//!
//! # #[cfg(not(feature = "tls"))] fn main() { }
//! # #[cfg(feature = "tls")]
//! fn main() {
//!     // setup the initializer for thread-local state
//!     COUNT.set(|| Cell::new(0));
//!
//!     // spin up many threads that call `function_to_measure`.
//!     let mut threads = vec![];
//!     for i in 0..10 {
//!         threads.push(thread::spawn(|| {
//!             // Thread IDs may be reusued, so we reset the state.
//!             COUNT.get().set(0);
//!             function_to_measure();
//!             COUNT.get().get()
//!         }));
//!     }
//!
//!     // retrieve the total
//!     let total: usize = threads.into_iter()
//!         .map(|t| t.join().unwrap())
//!         .sum();
//!
//!     assert_eq!(total, 10);
//! }
//! ```
//! ## Correctness
//!
//! `state` has been extensively vetted, manually and automatically, for soundness
//! and correctness. _All_ unsafe code, including in internal concurrency
//! primitives, `TypeMap`, and `InitCell` are exhaustively verified for pairwise
//! concurrency correctness and internal aliasing exclusion with `loom`.
//! Multithreading invariants, aliasing invariants, and other soundness properties
//! are verified with `miri`. Verification is run by the CI on every commit.
//!
//! ## Performance
//!
//! `state` is heavily tuned to perform optimally. `get{_local}` and
//! `set{_local}` calls to a `TypeMap` incur overhead due to type lookup.
//! `InitCell`, on the other hand, is optimal for global storage retrieval; it is
//! _slightly faster_ than accessing global state initialized through
//! `lazy_static!`, more so across many threads. `LocalInitCell` incurs slight
//! overhead due to thread lookup. However, `LocalInitCell` has no
//! synchronization overhead, so retrieval from `LocalInitCell` is faster than
//! through `InitCell` across many threads.
//!
//! Bear in mind that `state` allows global initialization at _any_ point in the
//! program. Other solutions, such as `lazy_static!` and `thread_local!` allow
//! initialization _only_ a priori. In other words, `state`'s abilities are a
//! superset of those provided by `lazy_static!` and `thread_local!` while being
//! more performant.
//!
//! ## When To Use
//!
//! You should avoid using global `state` as much as possible. Instead, thread
//! state manually throughout your program when feasible.

mod ident_hash;
mod cell;
mod init;
mod shim;

#[doc(hidden)]
pub mod type_map;

pub use type_map::TypeMap;
pub use cell::InitCell;

#[cfg(feature = "tls")] mod tls;
#[cfg(feature = "tls")] mod thread_local;
#[cfg(feature = "tls")] pub use tls::LocalInitCell;

/// Exports for use by loom tests but otherwise private.
#[cfg(loom)]
#[path = "."]
pub mod private {
    /// The `Init` type.
    pub mod init;
}
