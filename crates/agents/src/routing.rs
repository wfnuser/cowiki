use crate::protocol::HarnessRegistration;

/// Policy for routing tasks to the appropriate agent harness.
///
/// MVP implementation uses simple first-match routing.
/// Future enhancements can support complexity-based routing
/// (source type, token count, file count, priority).
pub struct HarnessRouter;

impl HarnessRouter {
    /// Select a harness for the given task type.
    ///
    /// # Arguments
    /// * `task_type` - The type of task (e.g., "shallow_compile", "deep_compile")
    /// * `harnesses` - Available harnesses that can handle this task type
    ///
    /// # Returns
    /// The selected harness, or `None` if no harnesses are available.
    ///
    /// # Strategy (MVP)
    /// Simple round-robin could be added later. Currently returns the first
    /// matching harness (which is sufficient when there's 1 harness per task type).
    pub fn route<'a>(
        task_type: &str,
        harnesses: &'a [&HarnessRegistration],
    ) -> Option<&'a HarnessRegistration> {
        if harnesses.is_empty() {
            tracing::warn!(%task_type, "no harnesses available for task type");
            return None;
        }

        // MVP: first-match — sufficient for single-harness-per-task-type
        let selected = harnesses.first().copied();
        if let Some(h) = selected {
            tracing::debug!(
                harness = %h.name,
                endpoint = %h.endpoint,
                %task_type,
                "routing: selected harness"
            );
        }
        selected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_harness(name: &str, task_type: &str) -> HarnessRegistration {
        HarnessRegistration {
            name: name.into(),
            task_type: task_type.into(),
            endpoint: format!("http://localhost:9000/{name}"),
            transport: crate::protocol::TransportType::Http,
            max_concurrency: 1,
        }
    }

    #[test]
    fn test_route_empty() {
        let harnesses: Vec<&HarnessRegistration> = vec![];
        assert!(HarnessRouter::route("shallow_compile", &harnesses).is_none());
    }

    #[test]
    fn test_route_first_match() {
        let h1 = make_harness("compile-simple", "shallow_compile");
        let h2 = make_harness("compile-advanced", "shallow_compile");
        let harnesses = vec![&h1, &h2];

        let result = HarnessRouter::route("shallow_compile", &harnesses);
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "compile-simple");
    }
}
