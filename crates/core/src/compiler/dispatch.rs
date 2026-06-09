//! Source-to-agent dispatch logic.
//!
//! Takes source folders and dispatches them to agent harnesses
//! for compilation into wiki pages, entities, and concepts.

use std::collections::HashMap;

use cowiki_agents::protocol::HarnessRegistration;

/// Result of dispatching a batch of sources to agents.
#[derive(Debug)]
pub struct DispatchPlan {
    /// Which harness each source folder should go to
    pub assignments: Vec<SourceAssignment>,
    /// Sources that were skipped (already compiled)
    pub skipped: Vec<String>,
}

#[derive(Debug)]
pub struct SourceAssignment {
    pub source_name: String,
    pub harness_name: String,
    pub endpoint: String,
}

/// Builds a dispatch plan from source folders and available harnesses.
///
/// MVP: assigns all sources to the first matching harness for `shallow_compile`.
/// Future: split large source folders across multiple agents, route by source type.
pub struct SourceDispatcher;

impl SourceDispatcher {
    /// Create a dispatch plan for the given sources.
    ///
    /// # Arguments
    /// * `sources` - Map of source folder name → content hash
    /// * `harnesses` - Available harnesses for shallow_compile
    /// * `already_compiled` - Map of source folder name → previously compiled content hash
    pub fn plan(
        sources: &HashMap<String, String>,
        harnesses: &[&HarnessRegistration],
        already_compiled: &HashMap<String, String>,
    ) -> DispatchPlan {
        let mut assignments = Vec::new();
        let mut skipped = Vec::new();

        // Select a harness (MVP: first available)
        let harness = match harnesses.first() {
            Some(h) => *h,
            None => {
                tracing::warn!("no harness available for source dispatch");
                return DispatchPlan {
                    assignments,
                    skipped: sources.keys().cloned().collect(),
                };
            }
        };

        for (source_name, content_hash) in sources {
            // Check if already compiled
            if let Some(compiled_hash) = already_compiled.get(source_name) {
                if compiled_hash == content_hash {
                    tracing::debug!(%source_name, "skipping — already compiled");
                    skipped.push(source_name.clone());
                    continue;
                }
            }

            assignments.push(SourceAssignment {
                source_name: source_name.clone(),
                harness_name: harness.name.clone(),
                endpoint: harness.endpoint.clone(),
            });
        }

        tracing::info!(
            assigned = assignments.len(),
            skipped = skipped.len(),
            "dispatch plan ready"
        );

        DispatchPlan {
            assignments,
            skipped,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_harness(name: &str) -> HarnessRegistration {
        HarnessRegistration {
            name: name.into(),
            task_type: "shallow_compile".into(),
            endpoint: format!("http://localhost:9000/{name}"),
            transport: cowiki_agents::protocol::TransportType::Http,
            max_concurrency: 1,
        }
    }

    #[test]
    fn test_skip_already_compiled() {
        let mut sources = HashMap::new();
        sources.insert("source-a".into(), "hash1".into());
        sources.insert("source-b".into(), "hash2".into());

        let h = make_harness("test");
        let harnesses = vec![&h];

        let mut compiled = HashMap::new();
        compiled.insert("source-a".into(), "hash1".into()); // already done

        let plan = SourceDispatcher::plan(&sources, &harnesses, &compiled);
        assert_eq!(plan.assignments.len(), 1);
        assert_eq!(plan.assignments[0].source_name, "source-b");
        assert_eq!(plan.skipped.len(), 1);
        assert_eq!(plan.skipped[0], "source-a");
    }

    #[test]
    fn test_all_new_sources() {
        let mut sources = HashMap::new();
        sources.insert("source-a".into(), "hash1".into());
        sources.insert("source-b".into(), "hash2".into());

        let h = make_harness("test");
        let harnesses = vec![&h];
        let compiled = HashMap::new();

        let plan = SourceDispatcher::plan(&sources, &harnesses, &compiled);
        assert_eq!(plan.assignments.len(), 2);
        assert!(plan.skipped.is_empty());
    }

    #[test]
    fn test_no_harness() {
        let mut sources = HashMap::new();
        sources.insert("source-a".into(), "hash1".into());

        let harnesses: Vec<&HarnessRegistration> = vec![];
        let compiled = HashMap::new();

        let plan = SourceDispatcher::plan(&sources, &harnesses, &compiled);
        assert!(plan.assignments.is_empty());
        assert_eq!(plan.skipped.len(), 1);
    }
}
