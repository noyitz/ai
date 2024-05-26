// SPDX-License-Identifier: LGPL-3.0-only
// Copyright (c) 2024 Shane Utt

//! Load-balancing strategy selection and dispatch.

use std::sync::Arc;

use praxis_core::{
    config::{LoadBalancerStrategy, ParameterisedStrategy, SimpleStrategy},
    health::ClusterHealthState,
};

use super::{
    consistent_hash::ConsistentHash, endpoint::WeightedEndpoint, least_connections::LeastConnections,
    round_robin::RoundRobin,
};
use crate::filter::HttpFilterContext;

// -----------------------------------------------------------------------------
// Strategy
// -----------------------------------------------------------------------------

/// Load-balancing strategy variant for a cluster.
pub(super) enum Strategy {
    /// Cycle through endpoints in order, respecting weights.
    RoundRobin(RoundRobin),

    /// Pick the endpoint with the fewest active requests.
    LeastConnections(LeastConnections),

    /// Hash a request attribute to a stable endpoint.
    ConsistentHash(ConsistentHash),
}

impl Strategy {
    /// Pick the next endpoint address, skipping unhealthy
    /// endpoints when health state is available.
    pub(super) fn select(&self, ctx: &HttpFilterContext<'_>, health: Option<&ClusterHealthState>) -> Arc<str> {
        match self {
            Self::RoundRobin(rr) => rr.select(health),
            Self::LeastConnections(lc) => lc.select(health),
            Self::ConsistentHash(ch) => ch.select(ctx, health),
        }
    }

    /// Called after a response arrives so that strategies that track in-flight
    /// request counts (e.g. `LeastConnections`) can decrement their counter.
    pub(super) fn release(&self, addr: &str) {
        if let Self::LeastConnections(lc) = self {
            lc.release(addr);
        }
    }
}

/// Create the appropriate strategy variant from the config.
pub(super) fn build_strategy(lb_strategy: &LoadBalancerStrategy, endpoints: Vec<WeightedEndpoint>) -> Strategy {
    match lb_strategy {
        LoadBalancerStrategy::Simple(SimpleStrategy::RoundRobin) => Strategy::RoundRobin(RoundRobin::new(endpoints)),
        LoadBalancerStrategy::Simple(SimpleStrategy::LeastConnections) => {
            Strategy::LeastConnections(LeastConnections::new(endpoints))
        },
        LoadBalancerStrategy::Parameterised(ParameterisedStrategy::ConsistentHash(opts)) => {
            Strategy::ConsistentHash(ConsistentHash::new(endpoints, opts.header.clone()))
        },
    }
}
