import dotenv from 'dotenv';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { ConfigError } from './error.js';

const DEFAULT_ENV_DIR = path.join(os.homedir(), '.cowiki-cli');
export const DEFAULT_ENV_PATH = path.join(DEFAULT_ENV_DIR, '.env');

export interface CliConfig {
  baseUrl: string;
  frontendUrl: string;
  apiKey?: string;
}

/**
 * Load config from ~/.cowiki-cli/.env and environment variables.
 * Priority: serverOverride > COWIKI_BASE_URL env var > .env > default
 */
export function loadConfig(serverOverride?: string): CliConfig {
  // Load .env from ~/.cowiki-cli/.env (silently skip if absent)
  dotenv.config({ path: DEFAULT_ENV_PATH, override: false });

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
 * Write or update entries in the .env file.
 * Preserves existing entries, adds/updates the given key-value pairs.
 * Default path: ~/.cowiki-cli/.env
 */
export function writeEnvFile(
  updates: Record<string, string>,
  envPath: string = DEFAULT_ENV_PATH,
): void {
  fs.mkdirSync(path.dirname(envPath), { recursive: true });

  let existing: Record<string, string> = {};
  if (fs.existsSync(envPath)) {
    const content = fs.readFileSync(envPath, 'utf-8');
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
