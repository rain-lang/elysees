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
//! [![Documentation](https://docs.rs/elysees/badge.svg)](https://docs.rs/elysees/)
//! [![Pipeline status](https://gitlab.com/rain-lang/elysees/badges/master/pipeline.svg)](https://gitlab.com/rain-lang/elysees)
//! [![codecov](https://codecov.io/gl/rain-lang/elysees/branch/master/graph/badge.svg)](https://codecov.io/gl/rain-lang/elysees)
//! [![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)
//!
//! Fork of Arc, now with more pointer tricks.

#![allow(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;
#[cfg(feature = "std")]
extern crate core;

#[cfg(feature = "serde")]
extern crate serde;
#[cfg(feature = "stable_deref_trait")]
extern crate stable_deref_trait;

use alloc::alloc::{alloc, dealloc, Layout};
use core::borrow::{Borrow, BorrowMut};
use core::cmp::Ordering;
use core::convert::From;
use core::fmt;
use core::hash::{Hash, Hasher};
use core::marker::PhantomData;
use core::mem;
use core::ops::{Deref, DerefMut};
use core::ptr;
use core::sync::atomic;
use core::sync::atomic::Ordering::{self as LoadOrdering, Acquire, Relaxed, Release};
use core::{isize, usize};

#[cfg(feature = "erasable")]
use erasable::{Erasable, ErasablePtr, ErasedPtr};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "stable_deref_trait")]
use stable_deref_trait::{CloneStableDeref, StableDeref};

mod arc;
mod borrow;
#[cfg(feature = "ptr-union")]
mod union;
mod unique;

pub use arc::*;
pub use borrow::*;
#[cfg(feature = "ptr-union")]
pub use union::*;
pub use unique::*;

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
