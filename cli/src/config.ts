import dotenv from 'dotenv';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { ConfigError } from './error.js';

const DEFAULT_ENV_DIR = path.join(os.homedir(), '.cowiki-cli');
/** User-level CLI config (KEY=VALUE lines). `.env` is the legacy name — read as a
 *  fallback so existing setups keep working; writes always go to `config`. */
export const DEFAULT_ENV_PATH = path.join(DEFAULT_ENV_DIR, 'config');
export const LEGACY_ENV_PATH = path.join(DEFAULT_ENV_DIR, '.env');

export interface CliConfig {
  baseUrl: string;
  frontendUrl: string;
  apiKey?: string;
}

/**
 * Load config from ~/.cowiki-cli/config (legacy ~/.cowiki-cli/.env as fallback)
 * and environment variables.
 * Priority: serverOverride > COWIKI_BASE_URL env var > config file > default
 */
export function loadConfig(serverOverride?: string): CliConfig {
  // Silently skip absent files; real env vars always win (override: false).
  const configPath = fs.existsSync(DEFAULT_ENV_PATH) ? DEFAULT_ENV_PATH : LEGACY_ENV_PATH;
  dotenv.config({ path: configPath, override: false });

  const baseUrl = serverOverride || process.env.COWIKI_BASE_URL || 'https://api.cowiki.app';
  const frontendUrl = process.env.COWIKI_FRONTEND_URL || 'http://localhost:5173';
  const apiKey = process.env.COWIKI_API_KEY || undefined;

  return { baseUrl, frontendUrl, apiKey };
}

/**
 * Validate that required config values are present.
 * Throws ConfigError if baseUrl is missing.
 */
export function validateConfig(config: CliConfig): void {
  if (!config.baseUrl) {
    throw new ConfigError('COWIKI_BASE_URL is not set');
  }
}

/**
 * Write or update entries in the config file.
 * Preserves existing entries (seeding from the legacy .env on first write so old
 * setups migrate transparently), adds/updates the given key-value pairs.
 * Default path: ~/.cowiki-cli/config
 */
export function writeEnvFile(
  updates: Record<string, string>,
  envPath: string = DEFAULT_ENV_PATH,
): void {
  fs.mkdirSync(path.dirname(envPath), { recursive: true });

  let existing: Record<string, string> = {};
  // First write at the new location migrates values from the legacy .env.
  const readPath =
    !fs.existsSync(envPath) && envPath === DEFAULT_ENV_PATH && fs.existsSync(LEGACY_ENV_PATH)
      ? LEGACY_ENV_PATH
      : envPath;
  if (fs.existsSync(readPath)) {
    const content = fs.readFileSync(readPath, 'utf-8');
    for (const line of content.split('\n')) {
      const trimmed = line.trim();
      if (trimmed && !trimmed.startsWith('#')) {
        const eqIdx = trimmed.indexOf('=');
        if (eqIdx > 0) {
          existing[trimmed.slice(0, eqIdx)] = trimmed.slice(eqIdx + 1);
        }
      }
    }
  }

  // Merge updates
  Object.assign(existing, updates);

  // Write back
  const lines = Object.entries(existing).map(([k, v]) => `${k}=${v}`);
  fs.writeFileSync(envPath, lines.join('\n') + '\n', { mode: 0o600 });
  // `mode` only applies when the file is created — an existing file keeps its old
  // (possibly world-readable) bits, so tighten explicitly: this file holds the API key.
  fs.chmodSync(envPath, 0o600);
}
