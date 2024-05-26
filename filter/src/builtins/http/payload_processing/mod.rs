// SPDX-License-Identifier: LGPL-3.0-only
// Copyright (c) 2024 Shane Utt

//! HTTP payload processing filters: compression, JSON body field extraction, etc.

mod compression;
pub(crate) mod compression_config;
mod json_body_field;

pub use compression::CompressionFilter;
pub use json_body_field::JsonBodyFieldFilter;
