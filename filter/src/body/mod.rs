// SPDX-License-Identifier: LGPL-3.0-only
// Copyright (c) 2024 Shane Utt

//! Body access declarations, buffering, and capability computation.

mod access;
mod buffer;
mod builder;
mod mode;

pub use access::BodyAccess;
pub use buffer::{BodyBuffer, BodyBufferOverflow};
pub use builder::BodyCapabilities;
pub use mode::BodyMode;
