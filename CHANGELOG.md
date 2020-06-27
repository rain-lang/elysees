# 0.1.0 (WIP)

- Added `get_count` method to obtain refcount of `Arc`, `ArcBorrow`, `ThinArc`, and `OffsetArc`
- Added `Hash` implementation to `ArcBorrow`, `ThinArc`, and `OffsetArc`, along with `HeaderSlice` and `HeaderWithLength`
- Added `Ord` implementation to `HeaderSlice`
- Changed default `Arc` implementation to `OffsetArc`, renaming the old `triomphe::Arc` to `ArcHandle`
- Expanded the API for `ArcBorrow` and the new `Arc`