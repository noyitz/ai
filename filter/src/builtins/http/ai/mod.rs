// SPDX-License-Identifier: LGPL-3.0-only
// Copyright (c) 2024 Shane Utt

//! AI filters for HTTP workloads.

mod inference;

pub use inference::ModelToHeaderFilter;
