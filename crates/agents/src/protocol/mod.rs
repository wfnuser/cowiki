//! Agent protocol adapters.
//!
//! Each protocol handles spawn + handshake + communication event loop.
//! Harnesses call into these — they don't know about protocol internals.
//!
//! Inspired by open-design's `acp.ts`, `pi-rpc.ts`, `copilot-stream.ts`.

pub mod acp;
pub mod pi_rpc;
