# Version 0.6.0 (May 25, 2023)

## Breaking Changes

* The MSRV is now 1.61.

* `Storage` was renamed to `InitCell`.

  The type was renamed to more accurately reflect its intended use and be more
  consistent with naming across the ecosystem. Several method names were also
  changed accordingly:

  - `Storage::get_or_set()` was renamed to `InitCell::get_or_init()`.
  - `Storage::set_or_get()` was renamed to `InitCell::set_or_init()`.

  Along with naming changes, the following improvements were made:

  - `InitCell<T>` can be constructed and used even with `T: !Sync + !Send`.
  - `InitCell<T>` impls `Send` even when `T: !Sync`.
  - Added `InitCell::{wait, reset, take, update}()` methods.

* `LocalStorage` was renamed to `LocalInitCell`.

  No other changes were made to the API.

* `Container` was renamed to `TypeMap`.

  The type was renamed to more accurately reflect its intended use and be more
  consistent with naming across the ecosystem. The following changes were also
  made:

  - `Container![]` is neither `Sync` nor `Send`.
  - An internal implementation detail was modified to more thoroughly ensure
    forwards-compatibility with Rust releases. This resulted in several uses of
    `unsafe` being removed at the cost of raising the MSRV to 1.61.

## General Improvements

* The crate now uses edition 2021.
* `LocalInitCell::try_get()` no longer panics when thread-local storage is not
  available.
