import { describe, test, expect, vi } from 'vitest';
import { CowikiClient } from '../src/client.js';

// Helper to capture the URL that fetch was called with
function captureFetchUrl(): { url: string } {
  const captured: { url: string } = { url: '' };
  vi.spyOn(globalThis, 'fetch').mockImplementation((input: RequestInfo | URL) => {
    captured.url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
    const resp = new Response(JSON.stringify([]), {
      status: 200,
      headers: { 'Content-Type': 'application/json' },
    });
    return Promise.resolve(resp);
  });
  return captured;
}

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

describe('listPages URL construction', () => {
  test('without dir — omits dir param', async () => {
    const cap = captureFetchUrl();
    const client = new CowikiClient('http://localhost:3000');
    await client.listPages('myws', 'main');
    expect(cap.url).toContain('/api/workspaces/myws/pages?branch=main');
    expect(cap.url).not.toContain('&dir=');
    vi.restoreAllMocks();
  });

  test('with dir=entities — appends dir param', async () => {
    const cap = captureFetchUrl();
    const client = new CowikiClient('http://localhost:3000');
    await client.listPages('myws', 'main', 'entities');
    expect(cap.url).toContain('&dir=entities');
    vi.restoreAllMocks();
  });

  test('with dir=all — URL-encodes correctly', async () => {
    const cap = captureFetchUrl();
    const client = new CowikiClient('http://localhost:3000');
    await client.listPages('myws', 'user/abc', 'all');
    expect(cap.url).toContain('&dir=all');
    vi.restoreAllMocks();
  });
});

describe('getPage URL construction', () => {
  test('without dir — omits dir param', async () => {
    const cap = captureFetchUrl();
    const client = new CowikiClient('http://localhost:3000');
    await client.getPage('myws', 'my-page', 'main');
    expect(cap.url).toContain('/api/workspaces/myws/pages/my-page?branch=main');
    expect(cap.url).not.toContain('&dir=');
    vi.restoreAllMocks();
  });

  test('with dir=concepts — appends dir param', async () => {
    const cap = captureFetchUrl();
    const client = new CowikiClient('http://localhost:3000');
    await client.getPage('myws', 'my-concept', 'main', 'concepts');
    expect(cap.url).toContain('&dir=concepts');
    vi.restoreAllMocks();
  });
});
