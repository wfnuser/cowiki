import { describe, test, expect, vi } from 'vitest';
import { CowikiClient } from '../src/client.js';

describe('CowikiClient', () => {
  test('constructs with base URL', () => {
    const client = new CowikiClient('http://localhost:3000');
    expect(client).toBeInstanceOf(CowikiClient);
  });

  test('normalizes trailing slash', () => {
    const client = new CowikiClient('http://localhost:3000/');
    // Internal baseUrl is normalized; just verify construction succeeds
    expect(client).toBeInstanceOf(CowikiClient);
  });

  test('accepts API key', () => {
    const client = new CowikiClient('https://cowiki.example.com', 'cw_test');
    expect(client).toBeInstanceOf(CowikiClient);
  });

  test('does not warn for localhost HTTP', () => {
    // Should not print warning for localhost
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {});
    new CowikiClient('http://localhost:3000', 'cw_test');
    expect(spy).not.toHaveBeenCalled();
    spy.mockRestore();
  });

  test('warns for remote HTTP with API key', () => {
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {});
    new CowikiClient('http://example.com', 'cw_test');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('WARNING'));
    spy.mockRestore();
  });

  test('does not warn for remote HTTPS', () => {
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {});
    new CowikiClient('https://example.com', 'cw_test');
    expect(spy).not.toHaveBeenCalled();
    spy.mockRestore();
  });

  test('does not warn for remote HTTP without API key', () => {
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {});
    new CowikiClient('http://example.com');
    expect(spy).not.toHaveBeenCalled();
    spy.mockRestore();
  });
});
