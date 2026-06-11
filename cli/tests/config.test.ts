import { describe, test, expect } from 'vitest';
import { loadConfig, validateConfig, writeEnvFile } from '../src/config.js';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';

describe('loadConfig', () => {
  test('returns defaults when no env vars set', () => {
    // Save and clear env vars
    const savedUrl = process.env.COWIKI_BASE_URL;
    const savedKey = process.env.COWIKI_API_KEY;
    delete process.env.COWIKI_BASE_URL;
    delete process.env.COWIKI_API_KEY;

    // Temporarily rename .env so dotenv doesn't load it
    const envPath = path.join(process.cwd(), '.env');
    const bakPath = path.join(process.cwd(), '.env.bak');
    let hadEnv = false;
    try {
      if (fs.existsSync(envPath)) {
        fs.renameSync(envPath, bakPath);
        hadEnv = true;
      }
      const config = loadConfig();
      expect(config.baseUrl).toBe('http://localhost:3000');
      expect(config.apiKey).toBeUndefined();
    } finally {
      if (hadEnv) fs.renameSync(bakPath, envPath);
      if (savedUrl) process.env.COWIKI_BASE_URL = savedUrl;
      if (savedKey) process.env.COWIKI_API_KEY = savedKey;
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
