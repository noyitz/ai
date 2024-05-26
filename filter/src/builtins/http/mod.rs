// SPDX-License-Identifier: LGPL-3.0-only
// Copyright (c) 2024 Shane Utt

//! HTTP protocol filters, organized by category.

mod ai;
pub(crate) mod net;
mod observability;
pub(crate) mod payload_processing;
mod security;
mod traffic_management;
mod transformation;

pub use ai::ModelToHeaderFilter;
pub use net::normalize_mapped_ipv4;
pub use observability::{AccessLogFilter, RequestIdFilter};
pub use payload_processing::{CompressionFilter, JsonBodyFieldFilter};
pub use security::{CorsFilter, ForwardedHeadersFilter, GuardrailsFilter, IpAclFilter};
pub use traffic_management::{
    LoadBalancerFilter, RateLimitFilter, RedirectFilter, RouterFilter, StaticResponseFilter, TimeoutFilter,
};
pub use transformation::{HeaderFilter, PathRewriteFilter, UrlRewriteFilter, normalize_rewritten_path};
