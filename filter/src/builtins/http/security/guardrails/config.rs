// SPDX-License-Identifier: LGPL-3.0-only
// Copyright (c) 2024 Shane Utt

//! Deserialized YAML configuration types for the guardrails filter.

use serde::Deserialize;

// -----------------------------------------------------------------------------
// Guardrails Constants
// -----------------------------------------------------------------------------

/// Default maximum body size for body inspection (1 MiB).
pub(super) const DEFAULT_MAX_BODY_BYTES: usize = 1_048_576;

/// Maximum allowed regex pattern length (characters).
pub(super) const MAX_REGEX_PATTERN_LEN: usize = 1024;

/// Maximum compiled regex automaton size (bytes, 1 MiB).
pub(super) const MAX_REGEX_SIZE: usize = 1_048_576;

// -----------------------------------------------------------------------------
// RuleConfig
// -----------------------------------------------------------------------------

/// Deserialized YAML config for a single guardrail rule.
#[derive(Debug, Deserialize)]
pub(super) struct RuleConfig {
    /// Header name (required when `target` is `"header"`).
    pub name: Option<String>,

    /// What to inspect: `"header"` or `"body"`.
    pub target: String,

    /// Literal substring match (case-sensitive).
    pub contains: Option<String>,

    /// Regex pattern match.
    pub pattern: Option<String>,

    /// Invert the match: reject when the content does NOT
    /// match. For negated header rules, a missing header
    /// also triggers rejection. Defaults to `false`.
    #[serde(default)]
    pub negate: bool,
}

// -----------------------------------------------------------------------------
// GuardrailsConfig
// -----------------------------------------------------------------------------

/// Deserialized YAML config for the guardrails filter.
#[derive(Debug, Deserialize)]
pub(super) struct GuardrailsConfig {
    /// List of rules to evaluate.
    pub rules: Vec<RuleConfig>,
}
