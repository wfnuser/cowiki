import { describe, test, expect } from 'vitest';
import { CliError, NetworkError, ApiError, ConfigError } from '../../src/error.js';

describe('error handling', () => {
  describe('ApiError', () => {
    test('formats 401 error', () => {
      const err = new ApiError(401, 'unauthorized');
      expect(err.message).toBe('API error (HTTP 401): unauthorized');
      expect(err.status).toBe(401);
      expect(err.exitCode).toBe(1);
    });

    test('formats 403 error', () => {
      const err = new ApiError(403, 'forbidden');
      expect(err.message).toContain('HTTP 403');
    });

    test('formats 500 error', () => {
      const err = new ApiError(500, 'internal error');
      expect(err.message).toContain('HTTP 500');
    });

    test('fromResponse parses JSON error body', async () => {
      const resp = new Response(JSON.stringify({ message: 'not found' }), { status: 404 });
      const err = await ApiError.fromResponse(resp);
      expect(err.status).toBe(404);
      expect(err.message).toContain('not found');
    });

    test('fromResponse handles non-JSON body', async () => {
      const resp = new Response('plain text', { status: 502, statusText: 'Bad Gateway' });
      const err = await ApiError.fromResponse(resp);
      expect(err.status).toBe(502);
      expect(err.message).toContain('Bad Gateway');
    });

    test('fromResponse uses error field if message absent', async () => {
      const resp = new Response(JSON.stringify({ error: 'something broke' }), { status: 500 });
      const err = await ApiError.fromResponse(resp);
      expect(err.message).toContain('something broke');
    });
  });

  describe('NetworkError', () => {
    test('formats network error', () => {
      const err = new NetworkError('connection refused');
      expect(err.message).toBe('connection refused');
      expect(err).toBeInstanceOf(CliError);
    });
  });

  describe('ConfigError', () => {
    test('formats config error', () => {
      const err = new ConfigError('missing API key');
      expect(err.message).toBe('Config error: missing API key');
      expect(err).toBeInstanceOf(CliError);
    });
  });

  describe('error hierarchy', () => {
    test('all errors are CliError and Error', () => {
      const errors = [
        new CliError('base'),
        new NetworkError('net'),
        new ApiError(400, 'api'),
        new ConfigError('cfg'),
      ];
      for (const e of errors) {
        expect(e).toBeInstanceOf(CliError);
        expect(e).toBeInstanceOf(Error);
      }
    });
  });
});
