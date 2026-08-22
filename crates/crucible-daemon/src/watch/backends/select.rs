//! Backend selection from a set of requirements.

use super::WatchBackend;
use crate::watch::error::{Error, Result};
use crate::watch::traits::BackendCapabilities;

/// Pick the best available backend for `requirements`.
///
/// A backend qualifies when its capability row covers every flag the caller
/// set, it runs on this platform, and its latency floor fits `max_latency_ms`.
/// Among the qualified backends the use case decides the order.
pub fn select_optimal_backend(requirements: &WatcherRequirements) -> Result<WatchBackend> {
    let suitable: Vec<WatchBackend> = WatchBackend::ALL
        .into_iter()
        .filter(|backend| backend.is_available())
        .filter(|backend| meets_requirements(*backend, requirements))
        .collect();

    requirements
        .use_case
        .priority_order()
        .into_iter()
        .find(|backend| suitable.contains(backend))
        .ok_or_else(|| Error::BackendUnavailable("No available backend meets requirements".into()))
}

fn meets_requirements(backend: WatchBackend, requirements: &WatcherRequirements) -> bool {
    let BackendCapabilities {
        recursive,
        fine_grained_events,
        multiple_paths,
        hot_reconfig,
        platforms: _,
    } = backend.capabilities();

    if requirements.recursive && !recursive {
        return false;
    }
    if requirements.fine_grained_events && !fine_grained_events {
        return false;
    }
    if requirements.multiple_paths && !multiple_paths {
        return false;
    }
    if requirements.hot_reconfig && !hot_reconfig {
        return false;
    }
    if let Some(max_latency) = requirements.max_latency_ms {
        if max_latency < backend.latency_floor_ms() {
            return false;
        }
    }
    true
}

/// Requirements for selecting a file watcher backend.
#[derive(Debug, Clone)]
pub struct WatcherRequirements {
    /// Whether recursive watching is needed.
    pub recursive: bool,
    /// Whether fine-grained events are needed.
    pub fine_grained_events: bool,
    /// Whether multiple paths need to be watched.
    pub multiple_paths: bool,
    /// Whether hot reconfiguration is needed.
    pub hot_reconfig: bool,
    /// Maximum acceptable latency in milliseconds.
    pub max_latency_ms: Option<u64>,
    /// Use case for the watcher.
    pub use_case: WatcherUseCase,
}

/// Use cases for file watching.
#[derive(Debug, Clone, PartialEq)]
pub enum WatcherUseCase {
    /// High-performance watching with minimal latency.
    HighPerformance,
    /// Low-frequency watching for background tasks.
    LowFrequency,
    /// Maximum compatibility across platforms.
    Compatibility,
    /// Integration with specific editors.
    EditorIntegration,
}

impl WatcherUseCase {
    /// The backends a use case prefers, best first.
    fn priority_order(&self) -> [WatchBackend; 3] {
        use WatchBackend::{Editor, Notify, Polling};
        match self {
            WatcherUseCase::HighPerformance => [Notify, Polling, Editor],
            WatcherUseCase::LowFrequency => [Editor, Polling, Notify],
            WatcherUseCase::Compatibility => [Polling, Notify, Editor],
            WatcherUseCase::EditorIntegration => [Editor, Notify, Polling],
        }
    }
}

impl Default for WatcherRequirements {
    fn default() -> Self {
        Self {
            recursive: true,
            fine_grained_events: true,
            multiple_paths: true,
            hot_reconfig: false,
            max_latency_ms: Some(100),
            use_case: WatcherUseCase::HighPerformance,
        }
    }
}

impl WatcherRequirements {
    /// Create requirements for high-performance use case.
    pub fn high_performance() -> Self {
        Self {
            max_latency_ms: Some(50),
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn high_performance_selects_notify() {
        let backend = select_optimal_backend(&WatcherRequirements::high_performance()).unwrap();
        assert_eq!(backend, WatchBackend::Notify);
    }

    #[test]
    fn latency_below_every_floor_selects_nothing() {
        let requirements = WatcherRequirements {
            max_latency_ms: Some(10),
            ..WatcherRequirements::default()
        };
        assert!(matches!(
            select_optimal_backend(&requirements),
            Err(Error::BackendUnavailable(_))
        ));
    }

    #[test]
    fn hot_reconfig_rules_out_notify() {
        let requirements = WatcherRequirements {
            hot_reconfig: true,
            fine_grained_events: false,
            max_latency_ms: None,
            ..WatcherRequirements::default()
        };
        let backend = select_optimal_backend(&requirements).unwrap();
        assert_eq!(backend, WatchBackend::Polling);
    }

    #[test]
    fn editor_integration_prefers_editor_when_not_recursive() {
        let requirements = WatcherRequirements {
            recursive: false,
            max_latency_ms: None,
            use_case: WatcherUseCase::EditorIntegration,
            ..WatcherRequirements::default()
        };
        let backend = select_optimal_backend(&requirements).unwrap();
        assert_eq!(backend, WatchBackend::Editor);
    }
}
