// SPDX-License-Identifier: LGPL-3.0-only
// Copyright (c) 2024 Shane Utt

//! Tests for the guardrails filter.

use bytes::Bytes;
use regex::Regex;

use super::{
    GuardrailsFilter,
    config::DEFAULT_MAX_BODY_BYTES,
    rule::{CompiledRule, RuleMatcher, RuleTarget},
};
use crate::{FilterAction, filter::HttpFilter};

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[test]
fn body_access_with_body_rules() {
    use crate::body::{BodyAccess, BodyMode};
    let f = make_filter(vec![body_contains("evil")]);
    assert_eq!(
        f.request_body_access(),
        BodyAccess::ReadOnly,
        "body rules need ReadOnly access"
    );
    assert_eq!(
        f.request_body_mode(),
        BodyMode::Buffer {
            max_bytes: DEFAULT_MAX_BODY_BYTES
        },
        "body rules need Buffer mode"
    );
}

#[test]
fn body_access_without_body_rules() {
    use crate::body::{BodyAccess, BodyMode};
    let f = make_filter(vec![header_contains("User-Agent", "bot")]);
    assert_eq!(
        f.request_body_access(),
        BodyAccess::None,
        "header-only rules need no body access"
    );
    assert_eq!(
        f.request_body_mode(),
        BodyMode::Stream,
        "header-only rules use Stream mode"
    );
}

#[tokio::test]
async fn header_contains_rejects_match() {
    let f = make_filter(vec![header_contains("user-agent", "bad-bot")]);
    let mut req = crate::test_utils::make_request(http::Method::GET, "/");
    req.headers.insert("user-agent", "bad-bot/1.0".parse().unwrap());
    let mut ctx = crate::test_utils::make_filter_context(&req);

    let action = f.on_request(&mut ctx).await.unwrap();
    assert!(
        matches!(action, FilterAction::Reject(r) if r.status == 401),
        "matching header should reject with 401"
    );
}

#[tokio::test]
async fn header_contains_allows_non_match() {
    let f = make_filter(vec![header_contains("user-agent", "bad-bot")]);
    let mut req = crate::test_utils::make_request(http::Method::GET, "/");
    req.headers.insert("user-agent", "good-bot/1.0".parse().unwrap());
    let mut ctx = crate::test_utils::make_filter_context(&req);

    let action = f.on_request(&mut ctx).await.unwrap();
    assert!(
        matches!(action, FilterAction::Continue),
        "non-matching header should continue"
    );
}

#[tokio::test]
async fn header_pattern_rejects_match() {
    let f = make_filter(vec![header_pattern("user-agent", r"bad-bot.*")]);
    let mut req = crate::test_utils::make_request(http::Method::GET, "/");
    req.headers.insert("user-agent", "bad-bot/2.0".parse().unwrap());
    let mut ctx = crate::test_utils::make_filter_context(&req);

    let action = f.on_request(&mut ctx).await.unwrap();
    assert!(
        matches!(action, FilterAction::Reject(r) if r.status == 401),
        "matching header regex should reject with 401"
    );
}

#[tokio::test]
async fn missing_header_does_not_match() {
    let f = make_filter(vec![header_contains("x-evil", "evilmonkey")]);
    let req = crate::test_utils::make_request(http::Method::GET, "/");
    let mut ctx = crate::test_utils::make_filter_context(&req);

    let action = f.on_request(&mut ctx).await.unwrap();
    assert!(
        matches!(action, FilterAction::Continue),
        "missing header should not trigger rule"
    );
}

#[tokio::test]
async fn body_contains_rejects_match() {
    let f = make_filter(vec![body_contains("DROP TABLE")]);
    let req = crate::test_utils::make_request(http::Method::POST, "/api");
    let mut ctx = crate::test_utils::make_filter_context(&req);

    let mut body = Some(Bytes::from_static(b"SELECT 1; DROP TABLE users;"));
    let action = f.on_request_body(&mut ctx, &mut body, true).await.unwrap();
    assert!(
        matches!(action, FilterAction::Reject(r) if r.status == 401),
        "matching body content should reject with 401"
    );
}

#[tokio::test]
async fn body_contains_allows_clean_content() {
    let f = make_filter(vec![body_contains("DROP TABLE")]);
    let req = crate::test_utils::make_request(http::Method::POST, "/api");
    let mut ctx = crate::test_utils::make_filter_context(&req);

    let mut body = Some(Bytes::from_static(b"SELECT 1 FROM users"));
    let action = f.on_request_body(&mut ctx, &mut body, true).await.unwrap();
    assert!(
        matches!(action, FilterAction::Continue),
        "clean body content should continue"
    );
}

#[tokio::test]
async fn body_pattern_rejects_match() {
    let f = make_filter(vec![body_pattern(r"(?i)drop\s+table")]);
    let req = crate::test_utils::make_request(http::Method::POST, "/api");
    let mut ctx = crate::test_utils::make_filter_context(&req);

    let mut body = Some(Bytes::from_static(b"drop  table users"));
    let action = f.on_request_body(&mut ctx, &mut body, true).await.unwrap();
    assert!(
        matches!(action, FilterAction::Reject(r) if r.status == 401),
        "matching body regex should reject with 401"
    );
}

#[tokio::test]
async fn none_body_continues() {
    let f = make_filter(vec![body_contains("evil")]);
    let req = crate::test_utils::make_request(http::Method::POST, "/api");
    let mut ctx = crate::test_utils::make_filter_context(&req);

    let mut body: Option<Bytes> = None;
    let action = f.on_request_body(&mut ctx, &mut body, true).await.unwrap();
    assert!(matches!(action, FilterAction::Continue), "None body should continue");
}

#[tokio::test]
async fn multiple_rules_first_match_rejects() {
    let f = make_filter(vec![
        header_contains("x-safe", "good"),
        header_contains("x-evil", "bad"),
    ]);
    let mut req = crate::test_utils::make_request(http::Method::GET, "/");
    req.headers.insert("x-evil", "bad-value".parse().unwrap());
    let mut ctx = crate::test_utils::make_filter_context(&req);

    let action = f.on_request(&mut ctx).await.unwrap();
    assert!(
        matches!(action, FilterAction::Reject(r) if r.status == 401),
        "any matching rule should trigger rejection"
    );
}

#[tokio::test]
async fn rejection_includes_body() {
    let f = make_filter(vec![header_contains("x-bad", "yes")]);
    let mut req = crate::test_utils::make_request(http::Method::GET, "/");
    req.headers.insert("x-bad", "yes".parse().unwrap());
    let mut ctx = crate::test_utils::make_filter_context(&req);

    let action = f.on_request(&mut ctx).await.unwrap();
    match action {
        FilterAction::Reject(r) => {
            assert_eq!(r.status, 401, "rejection status should be 401");
            assert_eq!(
                r.body.as_deref(),
                Some(b"Unauthorized".as_slice()),
                "rejection body should be 'Unauthorized'"
            );
        },
        _ => panic!("expected rejection"),
    }
}

#[tokio::test]
async fn negated_header_rejects_when_not_matching() {
    let f = make_filter(vec![header_not_contains("x-auth", "trusted")]);
    let mut req = crate::test_utils::make_request(http::Method::GET, "/");
    req.headers.insert("x-auth", "unknown".parse().unwrap());
    let mut ctx = crate::test_utils::make_filter_context(&req);

    let action = f.on_request(&mut ctx).await.unwrap();
    assert!(
        matches!(action, FilterAction::Reject(r) if r.status == 401),
        "negated rule should reject when header does not contain expected value"
    );
}

#[tokio::test]
async fn negated_header_allows_when_matching() {
    let f = make_filter(vec![header_not_contains("x-auth", "trusted")]);
    let mut req = crate::test_utils::make_request(http::Method::GET, "/");
    req.headers.insert("x-auth", "trusted-client".parse().unwrap());
    let mut ctx = crate::test_utils::make_filter_context(&req);

    let action = f.on_request(&mut ctx).await.unwrap();
    assert!(
        matches!(action, FilterAction::Continue),
        "negated rule should allow when header contains expected value"
    );
}

#[tokio::test]
async fn negated_header_rejects_when_header_missing() {
    let f = make_filter(vec![header_not_contains("x-auth", "trusted")]);
    let req = crate::test_utils::make_request(http::Method::GET, "/");
    let mut ctx = crate::test_utils::make_filter_context(&req);

    let action = f.on_request(&mut ctx).await.unwrap();
    assert!(
        matches!(action, FilterAction::Reject(r) if r.status == 401),
        "negated rule should reject when header is absent"
    );
}

#[tokio::test]
async fn negated_body_rejects_when_not_matching() {
    let f = make_filter(vec![body_not_contains("APPROVED")]);
    let req = crate::test_utils::make_request(http::Method::POST, "/api");
    let mut ctx = crate::test_utils::make_filter_context(&req);

    let mut body = Some(Bytes::from_static(b"some random content"));
    let action = f.on_request_body(&mut ctx, &mut body, true).await.unwrap();
    assert!(
        matches!(action, FilterAction::Reject(r) if r.status == 401),
        "negated body rule should reject when content does not match"
    );
}

#[tokio::test]
async fn negated_body_allows_when_matching() {
    let f = make_filter(vec![body_not_contains("APPROVED")]);
    let req = crate::test_utils::make_request(http::Method::POST, "/api");
    let mut ctx = crate::test_utils::make_filter_context(&req);

    let mut body = Some(Bytes::from_static(b"request APPROVED by admin"));
    let action = f.on_request_body(&mut ctx, &mut body, true).await.unwrap();
    assert!(
        matches!(action, FilterAction::Continue),
        "negated body rule should allow when content matches"
    );
}

#[tokio::test]
async fn negated_body_pattern_rejects_non_json() {
    let f = make_filter(vec![body_not_pattern(r"^\{.*\}$")]);
    let req = crate::test_utils::make_request(http::Method::POST, "/api");
    let mut ctx = crate::test_utils::make_filter_context(&req);

    let mut body = Some(Bytes::from_static(b"not json at all"));
    let action = f.on_request_body(&mut ctx, &mut body, true).await.unwrap();
    assert!(
        matches!(action, FilterAction::Reject(r) if r.status == 401),
        "negated pattern should reject body not matching expected shape"
    );
}

#[tokio::test]
async fn negated_body_pattern_allows_json() {
    let f = make_filter(vec![body_not_pattern(r"^\{.*\}$")]);
    let req = crate::test_utils::make_request(http::Method::POST, "/api");
    let mut ctx = crate::test_utils::make_filter_context(&req);

    let mut body = Some(Bytes::from_static(b"{\"key\":\"value\"}"));
    let action = f.on_request_body(&mut ctx, &mut body, true).await.unwrap();
    assert!(
        matches!(action, FilterAction::Continue),
        "negated pattern should allow body matching expected shape"
    );
}

// -----------------------------------------------------------------------------
// Test Utilities
// -----------------------------------------------------------------------------

/// Build a header-contains rule for testing.
fn header_contains(name: &str, needle: &str) -> CompiledRule {
    CompiledRule {
        target: RuleTarget::Header(name.to_owned()),
        matcher: RuleMatcher::Contains(needle.to_owned()),
        negate: false,
    }
}

/// Build a negated header-contains rule for testing.
fn header_not_contains(name: &str, needle: &str) -> CompiledRule {
    CompiledRule {
        target: RuleTarget::Header(name.to_owned()),
        matcher: RuleMatcher::Contains(needle.to_owned()),
        negate: true,
    }
}

/// Build a header-pattern rule for testing.
fn header_pattern(name: &str, re: &str) -> CompiledRule {
    CompiledRule {
        target: RuleTarget::Header(name.to_owned()),
        matcher: RuleMatcher::Pattern(Regex::new(re).unwrap()),
        negate: false,
    }
}

/// Build a body-contains rule for testing.
fn body_contains(needle: &str) -> CompiledRule {
    CompiledRule {
        target: RuleTarget::Body,
        matcher: RuleMatcher::Contains(needle.to_owned()),
        negate: false,
    }
}

/// Build a negated body-contains rule for testing.
fn body_not_contains(needle: &str) -> CompiledRule {
    CompiledRule {
        target: RuleTarget::Body,
        matcher: RuleMatcher::Contains(needle.to_owned()),
        negate: true,
    }
}

/// Build a body-pattern rule for testing.
fn body_pattern(re: &str) -> CompiledRule {
    CompiledRule {
        target: RuleTarget::Body,
        matcher: RuleMatcher::Pattern(Regex::new(re).unwrap()),
        negate: false,
    }
}

/// Build a negated body-pattern rule for testing.
fn body_not_pattern(re: &str) -> CompiledRule {
    CompiledRule {
        target: RuleTarget::Body,
        matcher: RuleMatcher::Pattern(Regex::new(re).unwrap()),
        negate: true,
    }
}

/// Build a filter from compiled rules.
fn make_filter(rules: Vec<CompiledRule>) -> GuardrailsFilter {
    let needs_body = rules.iter().any(|r| matches!(r.target, RuleTarget::Body));
    GuardrailsFilter { rules, needs_body }
}
