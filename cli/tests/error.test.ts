import { describe, test, expect } from 'vitest';
import { CliError, NetworkError, ApiError, ConfigError } from '../src/error.js';

describe('error classes', () => {
  test('ApiError 404 display', () => {
    const err = new ApiError(404, 'page not found');
    expect(err.message).toBe('API error (HTTP 404): page not found');
    expect(err.status).toBe(404);
    expect(err.name).toBe('ApiError');
  });

  test('ApiError 500 display', () => {
    const err = new ApiError(500, 'internal server error');
    expect(err.message).toBe('API error (HTTP 500): internal server error');
    expect(err.exitCode).toBe(1);
  });

  test('ConfigError display', () => {
    const err = new ConfigError('missing server url');
    expect(err.message).toBe('Config error: missing server url');
    expect(err.name).toBe('ConfigError');
  });

  test('NetworkError display', () => {
    const err = new NetworkError('connection refused');
    expect(err.message).toBe('connection refused');
    expect(err.name).toBe('NetworkError');
  });

  test('all errors extend CliError', () => {
    const api = new ApiError(400, 'bad request');
    const net = new NetworkError('timeout');
    const cfg = new ConfigError('bad config');
    expect(api).toBeInstanceOf(CliError);
    expect(net).toBeInstanceOf(CliError);
    expect(cfg).toBeInstanceOf(CliError);
    expect(api).toBeInstanceOf(Error);
  });
});
