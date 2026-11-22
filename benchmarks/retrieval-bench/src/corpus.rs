use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct EvalQuery {
    pub id: &'static str,
    pub category: &'static str,
    pub query: &'static str,
    pub supporting_pages: &'static [&'static str],
}

struct FixturePage {
    path: &'static str,
    body: &'static str,
}

const CORPUS_PAGES: &[FixturePage] = &[
    FixturePage {
        path: "index.md",
        body: "---\nokf_version: \"0.3\"\n---\n\n# Engineering Knowledge Base\n\n## Core Areas\n\n- [Architecture Overview](architecture/overview.md)\n- [Billing Service](services/billing.md)\n- [Auth Gateway](services/auth-gateway.md)\n- [Sync Engine](services/sync-engine.md)\n- [Incident Runbooks](runbooks/failover.md)\n",
    },
    // Deliberately high-degree hub page to stress hub-dampened link expansion.
    FixturePage {
        path: "architecture/overview.md",
        body: "---\ntype: Architecture\ntitle: Architecture Overview\nstatus: active\ntrust_tier: human_verified\nverified: true\n---\n\n# Architecture Overview\n\n## Topology\n\nCoWiki platform services connect through explicit contracts:\n- [Billing Service](../services/billing.md)\n- [Ledger Pipeline](../services/ledger.md)\n- [Auth Gateway](../services/auth-gateway.md)\n- [Token Vault](../security/token-vault.md)\n- [Sync Engine](../services/sync-engine.md)\n- [Conflict Resolver](../services/conflict-resolver.md)\n- [Search Indexer](../services/search-indexer.md)\n- [Telemetry Collector](../services/telemetry.md)\n- [Notification Dispatcher](../services/notifications.md)\n- [Export Worker](../services/export-worker.md)\n- [Retention Policy](../policies/retention-policy.md)\n- [Failover Playbook](../runbooks/failover.md)\n",
    },
    // Chain 1: Billing -> Ledger -> Retention Policy (2-hop Markdown + Wikilink)
    FixturePage {
        path: "services/billing.md",
        body: "---\ntype: Architecture\ntitle: Billing Service\nstatus: active\ntrust_tier: human_verified\nverified: true\nupdated_at: \"2026-10-15\"\n---\n\n# Billing Service\n\n## Charge Workflow\n\nSubscription invoices and metered usage charges flow from Billing Service into [Ledger Pipeline](ledger.md) before settlement.\n\n## Retry Semantics\n\nClient invoice retries carry a deterministic request key to prevent duplicate charges.\n",
    },
    FixturePage {
        path: "services/ledger.md",
        body: "---\ntype: Architecture\ntitle: Ledger Pipeline\nstatus: active\ntrust_tier: human_verified\nverified: true\nupdated_at: \"2026-10-18\"\n---\n\n# Ledger Pipeline\n\n## Posting Guarantees\n\nEvery posted double-entry journal commits atomically. Idempotency windows expire after 24 hours and archival follows [[Retention Policy]].\n\n## Reconciliation\n\nNightly batch reconciliation compares ledger snapshots against settlement bank statements.\n",
    },
    FixturePage {
        path: "policies/retention-policy.md",
        body: "---\ntype: Policy\ntitle: Retention Policy\nstatus: active\ntrust_tier: human_verified\nverified: true\nstale_after: \"2030-01-01\"\nupdated_at: \"2026-09-01\"\n---\n\n# Retention Policy\n\n## Financial Audit Records\n\nPosted financial ledger journals remain immutable in WORM object storage for exactly seven years before cryptographic erasure.\n\n## Ephemeral Diagnostics\n\nDebug trace payloads expire automatically after 14 days.\n",
    },
    // Chain 2: Auth Gateway -> Token Vault -> KMS Rotation (2-hop)
    FixturePage {
        path: "services/auth-gateway.md",
        body: "---\ntype: Architecture\ntitle: Auth Gateway\nstatus: active\ntrust_tier: human_verified\nverified: true\n---\n\n# Auth Gateway\n\n## Session Verification\n\nIncoming desktop OAuth bearer credentials are validated at the edge and delegated to [Token Vault](../security/token-vault.md) for envelope decryption.\n\n## Rate Limiting\n\nUnauthenticated login bursts are capped at 60 requests per minute per subnet.\n",
    },
    FixturePage {
        path: "security/token-vault.md",
        body: "---\ntype: Security\ntitle: Token Vault\nstatus: active\ntrust_tier: human_verified\nverified: true\n---\n\n# Token Vault\n\n## Envelope Encryption\n\nRefresh tokens are sealed with AES-256-GCM data keys wrapped by the root hierarchy defined in [KMS Key Rotation](kms-rotation.md).\n\n## Revocation\n\nCompromised sessions are invalidated within 3 seconds via Redis pub-sub fanout.\n",
    },
    FixturePage {
        path: "security/kms-rotation.md",
        body: "---\ntype: Security\ntitle: KMS Key Rotation\nstatus: active\ntrust_tier: human_verified\nverified: true\nstale_after: \"2030-06-01\"\n---\n\n# KMS Key Rotation\n\n## Automatic Schedule\n\nRoot hardware security module wrapping keys rotate every 90 days with a 15-day dual-decryption grace window.\n\n## Emergency Rekey\n\nIncident commanders can trigger zero-downtime rekeying via `kms-cli rotate --force`.\n",
    },
    // Chain 3: Sync Engine -> Conflict Resolver -> Merge Policy (2-hop)
    FixturePage {
        path: "services/sync-engine.md",
        body: "---\ntype: Architecture\ntitle: Sync Engine\nstatus: active\ntrust_tier: human_verified\nverified: true\n---\n\n# Sync Engine\n\n## Offline Replication\n\nLocal-first Markdown spaces push signed Git commits to cloud mirrors. Divergent branch tips hand off to [Conflict Resolver](conflict-resolver.md).\n\n## Transport Framing\n\nPackfile deltas stream over HTTP/2 with zstd compression.\n",
    },
    FixturePage {
        path: "services/conflict-resolver.md",
        body: "---\ntype: Architecture\ntitle: Conflict Resolver\nstatus: active\ntrust_tier: human_verified\nverified: true\n---\n\n# Conflict Resolver\n\n## Three-Way Tree Merge\n\nNon-overlapping Markdown files merge cleanly without touching working trees. Overlapping line edits escalate according to [Human Review Policy](../policies/human-review-policy.md).\n\n## Binary Files\n\nBinary attachments never attempt line-level merging.\n",
    },
    FixturePage {
        path: "policies/human-review-policy.md",
        body: "---\ntype: Policy\ntitle: Human Review Policy\nstatus: active\ntrust_tier: human_verified\nverified: true\n---\n\n# Human Review Policy\n\n## Conflict Escalation\n\nAny conflicting agent or remote edit is staged under an isolated review branch and requires explicit human diff approval before landing in Draft.\n\n## Trust Demotion\n\nMerging an AI-authored diff resets `verified: false` and sets `trust_tier: agent_draft`.\n",
    },
    // Chain 4: Search Indexer -> Embedding Cache -> Storage Quotas (2-hop)
    FixturePage {
        path: "services/search-indexer.md",
        body: "---\ntype: Architecture\ntitle: Search Indexer\nstatus: active\ntrust_tier: human_verified\nverified: true\n---\n\n# Search Indexer\n\n## Incremental Pipeline\n\nFile watcher batches re-parse modified Markdown files and persist vectors inside [Embedding Cache](embedding-cache.md).\n\n## Tokenization\n\nUnicode61 FTS5 indexes titles, section headings, and normalized body paragraphs.\n",
    },
    FixturePage {
        path: "services/embedding-cache.md",
        body: "---\ntype: Architecture\ntitle: Embedding Cache\nstatus: active\ntrust_tier: human_verified\nverified: true\n---\n\n# Embedding Cache\n\n## Content Addressing\n\nSection embeddings are keyed by `(model, sha256(text))` so unchanged paragraphs skip recomputation. Disk ceilings follow [[Storage Quotas]].\n\n## Eviction\n\nOrphan cache entries are pruned during nightly SQLite vacuum cycles.\n",
    },
    FixturePage {
        path: "policies/storage-quotas.md",
        body: "---\ntype: Policy\ntitle: Storage Quotas\nstatus: active\ntrust_tier: human_verified\nverified: true\n---\n\n# Storage Quotas\n\n## Local Index Budget\n\nDerived SQLite index files on desktop devices are soft-capped at 512 MB per Space with automatic WAL checkpoint truncation.\n\n## Attachment Limits\n\nSingle binary assets larger than 50 MB trigger an advisory warning.\n",
    },
    // Chain 5: Anchor-targeted 1-hop links (#quorum-recovery, #circuit-breaker, #cold-backup, #dlq-replay)
    FixturePage {
        path: "services/telemetry.md",
        body: "---\ntype: Architecture\ntitle: Telemetry Collector\nstatus: active\ntrust_tier: human_verified\nverified: true\n---\n\n# Telemetry Collector\n\n## Ingestion Pipeline\n\nStructured OpenTelemetry spans batch every 5 seconds. When downstream sinks saturate, operators execute [Failover Playbook](../runbooks/failover.md#circuit-breaker).\n\n## Sampling\n\nTail-based sampling retains 100% of error traces and 5% of healthy traces.\n",
    },
    FixturePage {
        path: "services/notifications.md",
        body: "---\ntype: Architecture\ntitle: Notification Dispatcher\nstatus: active\ntrust_tier: human_verified\nverified: true\n---\n\n# Notification Dispatcher\n\n## Webhook Delivery\n\nOutbound Slack and email webhooks retry with exponential backoff. Poison messages route to [Failover Playbook](../runbooks/failover.md#dlq-replay).\n",
    },
    FixturePage {
        path: "services/export-worker.md",
        body: "---\ntype: Architecture\ntitle: Export Worker\nstatus: active\ntrust_tier: human_verified\nverified: true\n---\n\n# Export Worker\n\n## Snapshot Packaging\n\nScheduled Space archives serialize Markdown trees into signed tarballs. Disaster restoration uses [Failover Playbook](../runbooks/failover.md#cold-backup).\n",
    },
    FixturePage {
        path: "services/cluster-coordinator.md",
        body: "---\ntype: Architecture\ntitle: Cluster Coordinator\nstatus: active\ntrust_tier: human_verified\nverified: true\n---\n\n# Cluster Coordinator\n\n## Raft Consensus\n\nMetadata replicas elect leaders using Raft leases. Split-brain partitions follow [Failover Playbook](../runbooks/failover.md#quorum-recovery).\n",
    },
    FixturePage {
        path: "runbooks/failover.md",
        body: "---\ntype: Runbook\ntitle: Failover Playbook\nstatus: active\ntrust_tier: human_verified\nverified: true\n---\n\n# Failover Playbook\n\n## Overview\n\nStandard emergency procedures for multi-region incidents.\n\n## Quorum Recovery\n\nIf two of five metadata voters fail simultaneously, promote the standby witness node in `us-west-2` and verify commit index parity before lifting write freezes.\n\n## Circuit Breaker\n\nTrip the collector shedding flag `TELEMETRY_SHED_NONCRITICAL=1` to drop debug spans and preserve error metrics under sink saturation.\n\n## DLQ Replay\n\nInspect dead-letter webhook envelopes with `notify-ctl dlq inspect`, fix invalid endpoints, then replay at 25 messages per second.\n\n## Cold Backup\n\nPull the latest GPG-verified tarball from archival bucket `cowiki-dr-vault` and verify SHA-256 manifests before rehydrating a Space.\n",
    },
    // Chain 6: Agent Sandbox -> Execution Gate -> Approval Matrix (2-hop)
    FixturePage {
        path: "services/agent-sandbox.md",
        body: "---\ntype: Architecture\ntitle: Agent Sandbox\nstatus: active\ntrust_tier: human_verified\nverified: true\n---\n\n# Agent Sandbox\n\n## Worktree Isolation\n\nEvery coding agent operates inside a dedicated Git worktree so Draft remains pristine. Tool invocation safety is checked by [Execution Gate](../security/execution-gate.md).\n",
    },
    FixturePage {
        path: "security/execution-gate.md",
        body: "---\ntype: Security\ntitle: Execution Gate\nstatus: active\ntrust_tier: human_verified\nverified: true\n---\n\n# Execution Gate\n\n## Capability Checks\n\nRead-only MCP queries execute automatically, while destructive filesystem actions consult [Approval Matrix](../policies/approval-matrix.md).\n",
    },
    FixturePage {
        path: "policies/approval-matrix.md",
        body: "---\ntype: Policy\ntitle: Approval Matrix\nstatus: active\ntrust_tier: human_verified\nverified: true\n---\n\n# Approval Matrix\n\n## Write Operations\n\nCross-space deletes and secret exports require two-factor operator confirmation with an explicit audit reason.\n",
    },
    // Chain 7: Mobile Sync -> Delta Compactor -> Battery Budget (2-hop)
    FixturePage {
        path: "services/mobile-sync.md",
        body: "---\ntype: Architecture\ntitle: Mobile Sync\nstatus: active\ntrust_tier: human_verified\nverified: true\n---\n\n# Mobile Sync\n\n## Background Wakeups\n\nHandheld clients coalesce edit patches using [Delta Compactor](delta-compactor.md) during metered cellular windows.\n",
    },
    FixturePage {
        path: "services/delta-compactor.md",
        body: "---\ntype: Architecture\ntitle: Delta Compactor\nstatus: active\ntrust_tier: human_verified\nverified: true\n---\n\n# Delta Compactor\n\n## Patch Squashing\n\nConsecutive keystroke diffs older than 10 minutes collapse into a single canonical snapshot obeying [[Battery Budget]].\n",
    },
    FixturePage {
        path: "policies/battery-budget.md",
        body: "---\ntype: Policy\ntitle: Battery Budget\nstatus: active\ntrust_tier: human_verified\nverified: true\n---\n\n# Battery Budget\n\n## Power Envelope\n\nBackground synchronization may consume at most 2% battery charge per 24-hour cycle and defers heavy indexing until plugged in.\n",
    },
    // Chain 8: Plugin Loader -> Manifest Verifier (1-hop anchor)
    FixturePage {
        path: "services/plugin-loader.md",
        body: "---\ntype: Architecture\ntitle: Plugin Loader\nstatus: active\ntrust_tier: human_verified\nverified: true\n---\n\n# Plugin Loader\n\n## Extension Startup\n\nCommunity plugins unpack into isolated bundles after signature checks described in [Manifest Verifier](../security/manifest-verifier.md#signature-pinning).\n",
    },
    FixturePage {
        path: "security/manifest-verifier.md",
        body: "---\ntype: Security\ntitle: Manifest Verifier\nstatus: active\ntrust_tier: human_verified\nverified: true\n---\n\n# Manifest Verifier\n\n## Schema Checks\n\nEvery plugin declares explicit filesystem and network scopes in `plugin.json`.\n\n## Signature Pinning\n\nProduction releases enforce Ed25519 publisher key pinning against the bundled root trust store before activating any hook.\n",
    },
    // Chain 9: Release Pipeline -> Canary Monitor (1-hop anchor)
    FixturePage {
        path: "services/release-pipeline.md",
        body: "---\ntype: Operations\ntitle: Release Pipeline\nstatus: active\ntrust_tier: human_verified\nverified: true\n---\n\n# Release Pipeline\n\n## Staged Rollouts\n\nDesktop updates roll out 5% -> 25% -> 100% guarded by [Canary Monitor](../runbooks/canary-monitor.md#rollback-triggers).\n",
    },
    FixturePage {
        path: "runbooks/canary-monitor.md",
        body: "---\ntype: Runbook\ntitle: Canary Monitor\nstatus: active\ntrust_tier: human_verified\nverified: true\n---\n\n# Canary Monitor\n\n## Dashboards\n\nWatch crash-free session ratio and startup p95 latency across macOS and Linux builds.\n\n## Rollback Triggers\n\nAutomatic halt engages whenever crash rate exceeds 0.4% or startup latency regresses by more than 250ms over a 15-minute window.\n",
    },
    // Morphological & Paraphrase target documents (designed to test vector similarity beyond strict FTS5 exact match)
    FixturePage {
        path: "guides/cryptographic-signing.md",
        body: "---\ntype: Guide\ntitle: Cryptographic Signing Guide\nstatus: active\ntrust_tier: human_verified\nverified: true\n---\n\n# Cryptographic Signing Guide\n\n## Attestation Mechanics\n\nArtifacts undergo deterministic hashing, asymmetric signature verification, and notarized timestamping prior to public distribution.\n",
    },
    FixturePage {
        path: "guides/observability-alerting.md",
        body: "---\ntype: Guide\ntitle: Observability Alerting Standards\nstatus: active\ntrust_tier: human_verified\nverified: true\n---\n\n# Observability Alerting Standards\n\n## Burn-Rate Paging\n\nError-budget alerting evaluates multi-window latency thresholds to minimize noisy on-call notifications and prevent operator fatigue.\n",
    },
    FixturePage {
        path: "guides/zero-downtime-migrations.md",
        body: "---\ntype: Guide\ntitle: Zero-Downtime Schema Migrations\nstatus: active\ntrust_tier: human_verified\nverified: true\n---\n\n# Zero-Downtime Schema Migrations\n\n## Expand and Contract\n\nRelational table upgrades dual-write compatible columns, backfill asynchronously in small batches, and drop legacy schemas only after readers converge.\n",
    },
    FixturePage {
        path: "guides/multilingual-tokenization.md",
        body: "---\ntype: Guide\ntitle: Multilingual Tokenization Design\nstatus: active\ntrust_tier: human_verified\nverified: true\n---\n\n# Multilingual Tokenization Design\n\n## CJK Segmentation\n\nMixed English and Chinese technical notes decompose into character bigrams alongside word stems for responsive offline querying.\n",
    },
    FixturePage {
        path: "guides/cache-invalidation.md",
        body: "---\ntype: Guide\ntitle: Distributed Cache Invalidation\nstatus: active\ntrust_tier: human_verified\nverified: true\n---\n\n# Distributed Cache Invalidation\n\n## Lease Versioning\n\nMonotonic generation counters reject stale cache repopulation writes during concurrent invalidation races.\n",
    },
    FixturePage {
        path: "guides/incident-postmortems.md",
        body: "---\ntype: Guide\ntitle: Blameless Incident Postmortems\nstatus: active\ntrust_tier: human_verified\nverified: true\n---\n\n# Blameless Incident Postmortems\n\n## Root-Cause Analysis\n\nPost-incident retrospectives document contributing systemic factors, timeline milestones, and verifiable preventive action items.\n",
    },
    // Governance disambiguation pages (retired, deprecated, stale, and redirect stubs)
    FixturePage {
        path: "policies/legacy-oauth-v1.md",
        body: "---\ntype: Policy\ntitle: OAuth Token Lifetime (Retired v1)\nstatus: retired\ntrust_tier: legacy\nverified: false\n---\n\n# OAuth Token Lifetime (Retired v1)\n\nLegacy bearer tokens lasted 365 days without refresh rotation. This policy is retired and must never be followed.\n",
    },
    FixturePage {
        path: "policies/oauth-token-lifetime.md",
        body: "---\ntype: Policy\ntitle: OAuth Token Lifetime Policy\nstatus: active\ntrust_tier: human_verified\nverified: true\nstale_after: \"2030-01-01\"\n---\n\n# OAuth Token Lifetime Policy\n\nAccess tokens expire in 15 minutes and refresh tokens rotate on every exchange with a 30-day hard ceiling.\n",
    },
    FixturePage {
        path: "guides/deploy-legacy-stub.md",
        body: "---\ntype: Guide\ntitle: Legacy Deployment Checklist\nstatus: deprecated\nredirect_to: guides/production-deployment.md\n---\n\n# Legacy Deployment Checklist\n\nMoved to the canonical deployment runbook.\n",
    },
    FixturePage {
        path: "guides/production-deployment.md",
        body: "---\ntype: Guide\ntitle: Production Deployment Runbook\nstatus: active\ntrust_tier: human_verified\nverified: true\nstale_after: \"2030-01-01\"\n---\n\n# Production Deployment Runbook\n\nSigned release containers deploy via immutable blue-green cutover after smoke assertions pass.\n",
    },
    FixturePage {
        path: "policies/backup-cadence-stale.md",
        body: "---\ntype: Policy\ntitle: Backup Cadence Policy (Old Draft)\nstatus: deprecated\ntrust_tier: agent_draft\nverified: false\ngenerated: true\nstale_after: \"2024-01-01\"\n---\n\n# Backup Cadence Policy (Old Draft)\n\nDatabase backups run weekly on Sunday nights.\n",
    },
    FixturePage {
        path: "policies/backup-cadence.md",
        body: "---\ntype: Policy\ntitle: Backup Cadence Policy\nstatus: active\ntrust_tier: human_verified\nverified: true\nstale_after: \"2030-01-01\"\n---\n\n# Backup Cadence Policy\n\nContinuous WAL archiving plus hourly incremental snapshots maintain a 5-minute RPO across all production clusters.\n",
    },
    FixturePage {
        path: "guides/rate-limit-retired.md",
        body: "---\ntype: Guide\ntitle: API Rate Limit Policy (Retired)\nstatus: retired\n---\n\n# API Rate Limit Policy (Retired)\n\nUnlimited unauthenticated requests allowed on legacy v0 endpoints.\n",
    },
    FixturePage {
        path: "guides/rate-limit-policy.md",
        body: "---\ntype: Guide\ntitle: API Rate Limit Policy\nstatus: active\ntrust_tier: human_verified\nverified: true\nstale_after: \"2030-01-01\"\n---\n\n# API Rate Limit Policy\n\nAuthenticated API clients receive 600 requests per minute with token-bucket burst smoothing.\n",
    },
];

pub const EVAL_QUERIES: &[EvalQuery] = &[
    // True 2-hop bridge queries (8): query names only the entry service/concept;
    // the intermediate bridge page and final policy page do NOT share the entry
    // entity name, so link-graph traversal is genuinely required.
    EvalQuery {
        id: "mh2_billing_retention",
        category: "multihop_2hop",
        query: "Billing Service compliance archival rule",
        supporting_pages: &[
            "services/billing.md",
            "services/ledger.md",
            "policies/retention-policy.md",
        ],
    },
    EvalQuery {
        id: "mh2_auth_kms_rotation",
        category: "multihop_2hop",
        query: "Auth Gateway edge credential protection schedule",
        supporting_pages: &[
            "services/auth-gateway.md",
            "security/token-vault.md",
            "security/kms-rotation.md",
        ],
    },
    EvalQuery {
        id: "mh2_sync_conflict_review",
        category: "multihop_2hop",
        query: "Sync Engine divergent branch escalation rules",
        supporting_pages: &[
            "services/sync-engine.md",
            "services/conflict-resolver.md",
            "policies/human-review-policy.md",
        ],
    },
    EvalQuery {
        id: "mh2_indexer_disk_quota",
        category: "multihop_2hop",
        query: "Search Indexer file watcher storage limit",
        supporting_pages: &[
            "services/search-indexer.md",
            "services/embedding-cache.md",
            "policies/storage-quotas.md",
        ],
    },
    EvalQuery {
        id: "mh2_agent_sandbox_approval",
        category: "multihop_2hop",
        query: "Agent Sandbox worktree tool safety confirmation",
        supporting_pages: &[
            "services/agent-sandbox.md",
            "security/execution-gate.md",
            "policies/approval-matrix.md",
        ],
    },
    EvalQuery {
        id: "mh2_mobile_sync_battery",
        category: "multihop_2hop",
        query: "Mobile Sync handheld metered cellular power cap",
        supporting_pages: &[
            "services/mobile-sync.md",
            "services/delta-compactor.md",
            "policies/battery-budget.md",
        ],
    },
    EvalQuery {
        id: "mh2_billing_settlement_worm",
        category: "multihop_2hop",
        query: "Billing Service metered invoice settlement rules",
        supporting_pages: &[
            "services/billing.md",
            "services/ledger.md",
            "policies/retention-policy.md",
        ],
    },
    EvalQuery {
        id: "mh2_auth_oauth_decryption",
        category: "multihop_2hop",
        query: "Auth Gateway desktop OAuth bearer decryption lifecycle",
        supporting_pages: &[
            "services/auth-gateway.md",
            "security/token-vault.md",
            "security/kms-rotation.md",
        ],
    },
    // 1-hop anchor-targeted queries (6): query identifies the service; the procedure
    // lives in a specific `#section-anchor` of the linked runbook/verifier.
    EvalQuery {
        id: "mh1_telemetry_shedding",
        category: "multihop_1hop_anchor",
        query: "Telemetry Collector OpenTelemetry sink saturation mitigation",
        supporting_pages: &["services/telemetry.md", "runbooks/failover.md"],
    },
    EvalQuery {
        id: "mh1_webhook_dlq",
        category: "multihop_1hop_anchor",
        query: "Notification Dispatcher Slack webhook exponential backoff failure handling",
        supporting_pages: &["services/notifications.md", "runbooks/failover.md"],
    },
    EvalQuery {
        id: "mh1_export_restore",
        category: "multihop_1hop_anchor",
        query: "Export Worker scheduled Space archive disaster recovery",
        supporting_pages: &["services/export-worker.md", "runbooks/failover.md"],
    },
    EvalQuery {
        id: "mh1_raft_quorum",
        category: "multihop_1hop_anchor",
        query: "Cluster Coordinator Raft lease split-brain partition procedure",
        supporting_pages: &["services/cluster-coordinator.md", "runbooks/failover.md"],
    },
    EvalQuery {
        id: "mh1_plugin_signing",
        category: "multihop_1hop_anchor",
        query: "Plugin Loader community extension bundle startup verification",
        supporting_pages: &["services/plugin-loader.md", "security/manifest-verifier.md"],
    },
    EvalQuery {
        id: "mh1_release_rollback",
        category: "multihop_1hop_anchor",
        query: "Release Pipeline staged desktop rollout halt condition",
        supporting_pages: &["services/release-pipeline.md", "runbooks/canary-monitor.md"],
    },
    // Morphological & paraphrase queries (6): strictly inflected/derived forms
    // that do NOT appear verbatim in the target page (testing dense vector cosine
    // beyond exact FTS5 unicode61 tokenization).
    EvalQuery {
        id: "para_crypto_attestation",
        category: "morphological_paraphrase",
        query: "cryptographically notarizing asymmetrically hashed distributions",
        supporting_pages: &["guides/cryptographic-signing.md"],
    },
    EvalQuery {
        id: "para_alert_fatigue",
        category: "morphological_paraphrase",
        query: "minimizing noisier paging alerted operators",
        supporting_pages: &["guides/observability-alerting.md"],
    },
    EvalQuery {
        id: "para_schema_backfill",
        category: "morphological_paraphrase",
        query: "upgrading backfilling asynchronously contracted tables",
        supporting_pages: &["guides/zero-downtime-migrations.md"],
    },
    EvalQuery {
        id: "para_cjk_segmentation",
        category: "morphological_paraphrase",
        query: "decomposing segmented tokenized bigramming queries",
        supporting_pages: &["guides/multilingual-tokenization.md"],
    },
    EvalQuery {
        id: "para_cache_versioning",
        category: "morphological_paraphrase",
        query: "invalidating repopulated monotonically versioned caches",
        supporting_pages: &["guides/cache-invalidation.md"],
    },
    EvalQuery {
        id: "para_retrospective_analysis",
        category: "morphological_paraphrase",
        query: "blamelessly documenting systemically retrospected preventions",
        supporting_pages: &["guides/incident-postmortems.md"],
    },
    // Governance disambiguation queries (4)
    EvalQuery {
        id: "gov_oauth_lifetime",
        category: "governance_disambiguated",
        query: "OAuth token lifetime policy bearer refresh expiration",
        supporting_pages: &["policies/oauth-token-lifetime.md"],
    },
    EvalQuery {
        id: "gov_redirect_deploy",
        category: "governance_disambiguated",
        query: "Legacy Deployment Checklist moved canonical",
        supporting_pages: &["guides/production-deployment.md"],
    },
    EvalQuery {
        id: "gov_backup_cadence",
        category: "governance_disambiguated",
        query: "Backup Cadence Policy database snapshots RPO",
        supporting_pages: &["policies/backup-cadence.md"],
    },
    EvalQuery {
        id: "gov_rate_limit",
        category: "governance_disambiguated",
        query: "API Rate Limit Policy requests per minute endpoints",
        supporting_pages: &["guides/rate-limit-policy.md"],
    },
];

pub fn write_eval_corpus(root: &Path) -> Result<(), String> {
    for page in CORPUS_PAGES {
        let target = root.join(page.path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(&target, page.body).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Generate a realistic interlinked multi-section OKF v0.3 corpus of `page_count`
/// pages for scaling, incremental update, and concurrent read benchmarks.
pub fn write_synthetic_space(root: &Path, page_count: usize) -> Result<Vec<String>, String> {
    fs::write(
        root.join("index.md"),
        "---\nokf_version: \"0.3\"\n---\n\n# Synthetic Benchmark Space\n\n## Overview\n\nGenerated multi-domain knowledge space.\n",
    )
    .map_err(|e| e.to_string())?;

    let domains = [
        "billing",
        "security",
        "storage",
        "sync",
        "search",
        "runtime",
        "telemetry",
        "deployment",
    ];
    let mut paths = Vec::with_capacity(page_count);

    for i in 0..page_count {
        let domain = domains[i % domains.len()];
        let next_i = (i + 1) % page_count;
        let next_domain = domains[next_i % domains.len()];
        let skip_i = (i + 7) % page_count;
        let skip_domain = domains[skip_i % domains.len()];

        let rel_path = format!("{domain}/doc-{i:04}.md");
        let target = root.join(&rel_path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let status = if i % 25 == 24 {
            "deprecated"
        } else if i % 40 == 39 {
            "retired"
        } else {
            "active"
        };
        let verified = status == "active" && i % 3 != 0;
        let trust_tier = if verified {
            "human_verified"
        } else {
            "agent_draft"
        };
        let body = format!(
            "---\ntype: Architecture\ntitle: \"{domain} Service Module {i:04}\"\nstatus: {status}\ntrust_tier: {trust_tier}\nverified: {verified}\nscope: {domain}\nupdated_at: \"2026-10-01\"\n---\n\n\
             # {domain} Service Module {i:04}\n\n\
             ## Architecture Overview\n\n\
             Module {i:04} coordinates {domain} workflows with deterministic checkpointing and connects to \
             [Module {next_i:04}](../{next_domain}/doc-{next_i:04}.md#operational-runbook) for downstream execution.\n\n\
             ## Consistency Model\n\n\
             State transitions validate sequence numbers {i:04} and delegate cross-domain policies to \
             [Module {skip_i:04}](../{skip_domain}/doc-{skip_i:04}.md).\n\n\
             ## Operational Runbook\n\n\
             When latency exceeds SLA threshold {sla}ms on worker shard {shard}, inspect queue depth and \
             enable backpressure flag `{domain_upper}_THROTTLE_{i:04}`.\n",
            sla = 120 + (i % 50) * 5,
            shard = i % 16,
            domain_upper = domain.to_ascii_uppercase(),
        );
        fs::write(&target, body).map_err(|e| e.to_string())?;
        paths.push(rel_path);
    }
    Ok(paths)
}
