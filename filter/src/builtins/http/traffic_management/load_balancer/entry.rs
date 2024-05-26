// SPDX-License-Identifier: LGPL-3.0-only
// Copyright (c) 2024 Shane Utt

//! Resolved cluster entry: strategy, connection options, and TLS config.

use std::sync::Arc;

use praxis_core::{
    config::Cluster,
    connectivity::{ConnectionOptions, Upstream},
};
use tracing::debug;

use super::{
    endpoint::build_weighted_endpoints,
    strategy::{Strategy, build_strategy},
};
use crate::filter::HttpFilterContext;

// -----------------------------------------------------------------------------
// ClusterEntry
// -----------------------------------------------------------------------------

/// Resolved state for a single cluster.
pub(super) struct ClusterEntry {
    /// Connection options derived from the cluster config, [`Arc`]-wrapped
    /// to avoid per-request cloning.
    pub(super) opts: Arc<ConnectionOptions>,

    /// The load-balancing strategy for this cluster.
    pub(super) strategy: Strategy,

    /// TLS settings for upstream connections. `None` means plain TCP.
    pub(super) tls: Option<praxis_core::config::ClusterTls>,
}

impl ClusterEntry {
    /// Build an [`Upstream`] from a selected address and request context.
    pub(super) fn build_upstream(&self, addr: Arc<str>, ctx: &HttpFilterContext<'_>) -> Upstream {
        let tls = self.tls.clone().map(|mut t| {
            if t.sni.is_none() {
                t.sni = ctx
                    .request
                    .headers
                    .get("host")
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_owned);
            }
            t
        });
        Upstream {
            address: addr,
            tls,
            connection: Arc::clone(&self.opts),
        }
    }
}

/// Build a [`ClusterEntry`] from a cluster definition.
pub(super) fn build_cluster_entry(cluster: &Cluster) -> ClusterEntry {
    let endpoints = build_weighted_endpoints(cluster);
    let total_weight: u32 = endpoints.iter().map(|ep| ep.weight).sum();
    debug!(
        cluster = %cluster.name,
        endpoints = endpoints.len(),
        total_weight,
        "cluster registered"
    );

    let strategy = build_strategy(&cluster.load_balancer_strategy, endpoints);
    ClusterEntry {
        opts: Arc::new(ConnectionOptions::from(cluster)),
        tls: cluster.tls.clone(),
        strategy,
    }
}
