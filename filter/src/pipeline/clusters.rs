// SPDX-License-Identifier: MIT
// Copyright (c) 2024 Shane Utt

//! YAML cluster name extraction from filter entries.

use std::collections::HashSet;

use praxis_core::config::FilterEntry;

// -----------------------------------------------------------------------------
// YAML Cluster Extraction
// -----------------------------------------------------------------------------

/// Extract cluster names from router filter entries' YAML config.
pub(super) fn extract_router_clusters(entries: &[FilterEntry]) -> HashSet<String> {
    let mut clusters = HashSet::new();
    for entry in entries {
        if entry.filter_type != "router" {
            continue;
        }
        let Some(routes) = entry.config.get("routes") else {
            continue;
        };
        let Some(routes) = routes.as_sequence() else {
            continue;
        };
        for route in routes {
            if let Some(cluster) = route.get("cluster").and_then(|v| v.as_str()) {
                clusters.insert(cluster.to_owned());
            }
        }
    }
    clusters
}

/// Extract cluster names from `load_balancer` filter entries' YAML config.
pub(super) fn extract_lb_clusters(entries: &[FilterEntry]) -> HashSet<String> {
    let mut clusters = HashSet::new();
    for entry in entries {
        if entry.filter_type != "load_balancer" {
            continue;
        }
        let Some(cluster_list) = entry.config.get("clusters") else {
            continue;
        };
        let Some(cluster_list) = cluster_list.as_sequence() else {
            continue;
        };
        for cluster in cluster_list {
            if let Some(name) = cluster.get("name").and_then(|v| v.as_str()) {
                clusters.insert(name.to_owned());
            }
        }
    }
    clusters
}
