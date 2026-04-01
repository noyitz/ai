// SPDX-License-Identifier: LGPL-3.0-only
// Copyright (c) 2024 Shane Utt

//! Built-in proxy configuration for Praxis.

use std::path::PathBuf;

use super::ProxyConfig;

// -----------------------------------------------------------------------------
// PraxisConfig
// -----------------------------------------------------------------------------

/// Built-in [`ProxyConfig`] for Praxis.
///
/// ```
/// use std::path::PathBuf;
///
/// use benchmarks::proxy::{PraxisConfig, ProxyConfig};
///
/// let cfg = PraxisConfig {
///     config: PathBuf::from("/tmp/test.yaml"),
///     address: "127.0.0.1:8080".into(),
///     image: None,
/// };
/// assert_eq!(cfg.name(), "praxis");
/// ```
#[derive(Debug)]
pub struct PraxisConfig {
    /// Path to the Praxis YAML config file.
    pub config: PathBuf,

    /// Listen address (defaults to "127.0.0.1:8080").
    pub address: String,

    /// Optional Docker image override. When set, runs via docker instead of cargo.
    pub image: Option<String>,
}

impl ProxyConfig for PraxisConfig {
    fn name(&self) -> &str {
        "praxis"
    }

    fn listen_address(&self) -> &str {
        &self.address
    }

    fn start_command(&self) -> (String, Vec<String>) {
        match &self.image {
            Some(image) => docker_command(&self.config, image),
            None => cargo_command(&self.config),
        }
    }

    fn config_path(&self) -> &std::path::Path {
        &self.config
    }

    fn container_name(&self) -> Option<&str> {
        if self.image.is_some() {
            Some("praxis-bench-praxis")
        } else {
            None
        }
    }
}

/// Build a Docker run command for the Praxis benchmark.
fn docker_command(config: &std::path::Path, image: &str) -> (String, Vec<String>) {
    let config_abs = std::fs::canonicalize(config).unwrap_or_else(|_| config.to_path_buf());
    (
        "docker".into(),
        vec![
            "run".into(),
            "--rm".into(),
            "--name".into(),
            "praxis-bench-praxis".into(),
            "--network".into(),
            "host".into(),
            "--cpus=4.0".into(),
            "--memory=2g".into(),
            "-v".into(),
            format!("{}:/etc/praxis/config.yaml:ro", config_abs.display()),
            image.into(),
        ],
    )
}

/// Build a cargo run command for the Praxis benchmark.
fn cargo_command(config: &std::path::Path) -> (String, Vec<String>) {
    (
        "cargo".into(),
        vec![
            "run".into(),
            "--release".into(),
            "-p".into(),
            "praxis".into(),
            "--".into(),
            "-c".into(),
            config.display().to_string(),
        ],
    )
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn praxis_config_command() {
        let config = PraxisConfig {
            config: PathBuf::from("/tmp/test.yaml"),
            address: "127.0.0.1:9090".into(),
            image: None,
        };

        assert_eq!(config.name(), "praxis");
        assert_eq!(config.listen_address(), "127.0.0.1:9090");

        let (cmd, args) = config.start_command();
        assert_eq!(cmd, "cargo");
        assert!(
            args.contains(&"--release".to_owned()),
            "start command should include --release flag"
        );
        assert!(
            args.contains(&"-c".to_owned()),
            "start command should include -c config flag"
        );
    }
}
