import { describe, test, expect } from 'vitest';
import { loadConfig, validateConfig, writeEnvFile } from '../src/config.js';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';

describe('loadConfig', () => {
  test('env vars take precedence (deterministic regardless of ~/.cowiki-cli/.env)', () => {
    // The pure fallback (https://api.cowiki.app) can't be asserted hermetically:
    // loadConfig reads the developer's real ~/.cowiki-cli/.env via dotenv. Setting
    // the vars explicitly is deterministic because dotenv uses override: false.
    const savedUrl = process.env.COWIKI_BASE_URL;
    const savedKey = process.env.COWIKI_API_KEY;
    try {
      process.env.COWIKI_BASE_URL = 'http://localhost:3000';
      process.env.COWIKI_API_KEY = 'cw_hermetic_test';
      const config = loadConfig();
      expect(config.baseUrl).toBe('http://localhost:3000');
      expect(config.apiKey).toBe('cw_hermetic_test');
    } finally {
      if (savedUrl) process.env.COWIKI_BASE_URL = savedUrl;
      else delete process.env.COWIKI_BASE_URL;
      if (savedKey) process.env.COWIKI_API_KEY = savedKey;
      else delete process.env.COWIKI_API_KEY;
    }
  });

  test('reads env vars', () => {
    process.env.COWIKI_BASE_URL = 'https://cowiki.example.com';
    process.env.COWIKI_API_KEY = 'cw_test123';

    const config = loadConfig();
    expect(config.baseUrl).toBe('https://cowiki.example.com');
    expect(config.apiKey).toBe('cw_test123');

    delete process.env.COWIKI_BASE_URL;
    delete process.env.COWIKI_API_KEY;
  });
});

describe('validateConfig', () => {
  test('passes with valid config', () => {
    expect(() => validateConfig({ baseUrl: 'http://localhost:3000' })).not.toThrow();
  });

  test('throws on empty baseUrl', () => {
    expect(() => validateConfig({ baseUrl: '' })).toThrow('COWIKI_BASE_URL');
  });
});

describe('writeEnvFile', () => {
  test('writes .env file', () => {
    const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'cowiki-test-'));
    const envPath = path.join(tmpDir, '.env');
    try {
      writeEnvFile(
        { COWIKI_BASE_URL: 'http://localhost:9999', COWIKI_API_KEY: 'cw_xxx' },
        envPath,
      );
      const content = fs.readFileSync(envPath, 'utf-8');
      expect(content).toContain('COWIKI_BASE_URL=http://localhost:9999');
      expect(content).toContain('COWIKI_API_KEY=cw_xxx');
    } finally {
      fs.rmSync(tmpDir, { recursive: true, force: true });
    }
  });
});
