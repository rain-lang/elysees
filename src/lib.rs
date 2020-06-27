// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! [![crates.io](https://img.shields.io/crates/v/elysees)](https://crates.io/crates/elysees)
//! [![Downloads](https://img.shields.io/crates/d/elysees)](https://crates.io/crates/elysees)
//! Fork of Arc, now with more pointer tricks.

#![allow(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;
#[cfg(feature = "std")]
extern crate core;

#[macro_use]
extern crate memoffset;
#[cfg(feature = "serde")]
extern crate serde;
#[cfg(feature = "stable_deref_trait")]
extern crate stable_deref_trait;

mod arc;
mod arc_borrow;
mod arc_handle;
mod arc_union;
mod header;
mod thin_arc;
mod unique_arc;

pub use arc::*;
pub use arc_borrow::*;
pub use arc_handle::*;
pub use arc_union::*;
pub use header::*;
pub use thin_arc::*;
pub use unique_arc::*;

#[cfg(feature = "std")]
use std::process::abort;

// `no_std`-compatible abort by forcing a panic while already panicing.
#[cfg(not(feature = "std"))]
#[cold]
fn abort() -> ! {
    struct PanicOnDrop;
    impl Drop for PanicOnDrop {
        fn drop(&mut self) {
            panic!()
        }
    }
    let _double_panicer = PanicOnDrop;
    panic!();
}
