# 0.2.2

- Fixed failing `--no-default-features` build

# 0.2.1

- Added `Arbitrary` implementation for `Arc` and `ArcBox`

# 0.2.0

- Remove erroneous `Borrow` implementation for `Arc`, `ArcBorrow`, and `ArcBox`

# 0.1.2

- Fix undefined behaviour in MIRI
    - Fix `ArcInner` layout calculation
    - Change representation of `ArcBorrow` to use a `NonNull`
- Add `from_raw`, `into_raw` API functions to `ArcBorrow`

# 0.1.1

- Edit documentation, etc.

# 0.1.0

Initial release