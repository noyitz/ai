// SPDX-License-Identifier: LGPL-3.0-only
// Copyright (c) 2024 Shane Utt

//! Rejects requests matching string or regex guardrail rules.

mod config;
mod filter;
mod rule;

#[cfg(test)]
mod tests;

pub use self::filter::GuardrailsFilter;
