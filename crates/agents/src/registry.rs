use std::collections::HashMap;

use crate::protocol::HarnessRegistration;

/// Registry of available agent harnesses.
///
/// Harnesses are external HTTP/gRPC services that execute agent tasks.
/// The registry maps harness names and task types to their endpoints.
#[derive(Default)]
pub struct HarnessRegistry {
    harnesses: HashMap<String, HarnessRegistration>,
}

impl HarnessRegistry {
    pub fn new() -> Self {
        Self {
            harnesses: HashMap::new(),
        }
    }

    /// Register a new harness. Replaces any existing harness with the same name.
    pub fn register(&mut self, harness: HarnessRegistration) {
        tracing::info!(
            name = %harness.name,
            task_type = %harness.task_type,
            endpoint = %harness.endpoint,
            "registering agent harness"
        );
        self.harnesses.insert(harness.name.clone(), harness);
    }

    /// Remove a harness by name.
    pub fn unregister(&mut self, name: &str) -> Option<HarnessRegistration> {
        let removed = self.harnesses.remove(name);
        if removed.is_some() {
            tracing::info!(%name, "unregistered agent harness");
        }
        removed
    }

    /// Get a harness by name.
    pub fn get(&self, name: &str) -> Option<&HarnessRegistration> {
        self.harnesses.get(name)
    }

    /// Get all harnesses that can handle a given task type.
    pub fn get_for_task(&self, task_type: &str) -> Vec<&HarnessRegistration> {
        self.harnesses
            .values()
            .filter(|h| h.task_type == task_type)
            .collect()
    }

    /// List all registered harnesses.
    pub fn list_all(&self) -> Vec<&HarnessRegistration> {
        self.harnesses.values().collect()
    }

    /// Check if a harness exists for the given task type.
    pub fn has_task_type(&self, task_type: &str) -> bool {
        self.harnesses.values().any(|h| h.task_type == task_type)
    }

    /// Get the count of registered harnesses.
    pub fn len(&self) -> usize {
        self.harnesses.len()
    }

    pub fn is_empty(&self) -> bool {
        self.harnesses.is_empty()
    }
}

impl HarnessRegistry {
    /// Create a registry pre-populated with default harnesses for MVP.
    ///
    /// Default harnesses:
    /// - `compile-simple`: Handles `shallow_compile` tasks on port 9100
    /// - `deep-compile`: Handles `deep_compile` tasks on port 9101
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.register(HarnessRegistration {
            name: "compile-simple".into(),
            task_type: crate::protocol::task_type::SHALLOW_COMPILE.into(),
            endpoint: "http://localhost:9100/agent/run".into(),
            transport: crate::protocol::TransportType::Http,
            max_concurrency: 4,
        });
        registry.register(HarnessRegistration {
            name: "deep-compile".into(),
            task_type: crate::protocol::task_type::DEEP_COMPILE.into(),
            endpoint: "http://localhost:9101/agent/run".into(),
            transport: crate::protocol::TransportType::Http,
            max_concurrency: 1,
        });
        registry
    }
}
