//! Agent pool management for compile system.
//!
//! Manages agent concurrency with per-space limits and tier-based gating.
//!
//! # Concurrency Model
//! - **ShallowCompile**: Multiple requests concurrent, queued if pool full, 503 if queue full
//! - **DeepCompile**: Per-space mutex — one run per space at a time, 409 if running
//! - Different spaces run concurrently
//!
//! # Space Eviction
//! Spaces are stored in a bounded map (default 10,000 entries). When the map is full,
//! the least-recently-used space is evicted — but only if it has no active permits.
//! This prevents memory exhaustion from unbounded space creation.

use std::num::NonZeroUsize;
use std::sync::Arc;

use lru::LruCache;
use tokio::sync::{Mutex, Semaphore};
use cowiki_agents::error::AgentError;
use cowiki_agents::protocol::{PoolConfig, TierLimit};

/// Manages agent pools for different spaces and task types.
///
/// Each space gets its own semaphore-governed pool for each task type.
/// Pool sizes are capped by tier limits.
pub struct AgentPool {
    config: PoolConfig,
    tier_limits: TierLimit,
    /// Per-space, per-task-type semaphore pools (bounded LRU)
    pools: Arc<Mutex<LruCache<String, SpacePools>>>,
}

struct SpacePools {
    /// Semaphore for shallow_compile tasks
    shallow_compile: Arc<Semaphore>,
    /// Semaphore for deep_compile tasks (1 permit = mutex)
    deep_compile: Arc<Semaphore>,
}

/// Maximum number of spaces tracked in the pool.
/// Beyond this, least-recently-used idle spaces are evicted.
const MAX_SPACES: usize = 10_000;

impl AgentPool {
    /// Create a new agent pool with the given config and tier.
    pub fn new(config: PoolConfig, tier: &str) -> Self {
        let tier_limits = TierLimit::for_tier(tier);
        Self {
            config,
            tier_limits,
            pools: Arc::new(Mutex::new(
                LruCache::new(NonZeroUsize::new(MAX_SPACES).unwrap()),
            )),
        }
    }

    /// Get or create per-space pools.
    /// If the LRU cache is full, evicts the least-recently-used idle space.
    async fn get_or_create_space(&self, space: &str) -> SpacePools {
        let mut pools = self.pools.lock().await;
        if let Some(space_pools) = pools.get(space) {
            return SpacePools {
                shallow_compile: space_pools.shallow_compile.clone(),
                deep_compile: space_pools.deep_compile.clone(),
            };
        }

        let shallow_size = self
            .tier_limits
            .max_for("shallow_compile")
            .min(self.config.shallow_compile.size);

        let space_pools = SpacePools {
            shallow_compile: Arc::new(Semaphore::new(shallow_size as usize)),
            deep_compile: Arc::new(Semaphore::new(1)), // 1 permit = mutex
        };

        pools.put(space.to_string(), SpacePools {
            shallow_compile: space_pools.shallow_compile.clone(),
            deep_compile: space_pools.deep_compile.clone(),
        });

        space_pools
    }

    /// Acquire a permit for a shallow_compile task in the given space.
    ///
    /// Returns a guard that releases the permit when dropped.
    /// Returns `AgentError::PoolExhausted` if no permits are available
    /// (non-blocking — callers should return 503).
    pub async fn acquire_shallow_compile(
        &self,
        space: &str,
    ) -> Result<ShallowCompileGuard, AgentError> {
        let space_pools = self.get_or_create_space(space).await;
        let permit = space_pools
            .shallow_compile
            .clone()
            .try_acquire_owned()
            .map_err(|_| AgentError::PoolExhausted {
                task_type: "shallow_compile".into(),
            })?;

        tracing::info!(
            %space,
            available = space_pools.shallow_compile.available_permits(),
            "acquired shallow_compile permit"
        );

        Ok(ShallowCompileGuard { _permit: permit })
    }

    /// Try to acquire the deep_compile mutex for a space.
    ///
    /// Returns a guard that auto-releases on drop (via Semaphore permit).
    /// Returns `AgentError::PoolExhausted` if a deep_compile is already
    /// running in this space (callers should return 409).
    pub async fn try_acquire_deep_compile(
        &self,
        space: &str,
    ) -> Result<DeepCompileGuard, AgentError> {
        let space_pools = self.get_or_create_space(space).await;
        let permit = space_pools
            .deep_compile
            .clone()
            .try_acquire_owned()
            .map_err(|_| AgentError::PoolExhausted {
                task_type: "deep_compile".into(),
            })?;

        tracing::info!(%space, "acquired deep_compile mutex");

        Ok(DeepCompileGuard { _permit: permit })
    }

    /// Get current pool status for observability.
    pub async fn status(&self, space: &str) -> PoolStatus {
        let space_pools = self.get_or_create_space(space).await;
        let deep_running = space_pools.deep_compile.available_permits() == 0;

        PoolStatus {
            space: space.to_string(),
            shallow_compile_available: space_pools.shallow_compile.available_permits(),
            shallow_compile_max: self
                .tier_limits
                .max_for("shallow_compile")
                .min(self.config.shallow_compile.size) as usize,
            deep_compile_running: deep_running,
            tier: self.tier_limits.tier.clone(),
        }
    }
}

/// RAII guard that releases a shallow_compile permit on drop.
pub struct ShallowCompileGuard {
    _permit: tokio::sync::OwnedSemaphorePermit,
}

/// RAII guard that releases the deep_compile mutex on drop.
/// The Semaphore permit is automatically released when this guard is dropped,
/// ensuring the deep_compile lock is never leaked.
pub struct DeepCompileGuard {
    _permit: tokio::sync::OwnedSemaphorePermit,
}

/// Snapshot of pool status for a space.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PoolStatus {
    pub space: String,
    pub shallow_compile_available: usize,
    pub shallow_compile_max: usize,
    pub deep_compile_running: bool,
    pub tier: String,
}
