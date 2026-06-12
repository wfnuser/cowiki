import { describe, test, expect } from 'vitest';
import { urlencode } from '../src/utils/urlencode.js';

describe('urlencode', () => {
  test('simple string', () => {
    expect(urlencode('hello')).toBe('hello');
  });

  test('spaces', () => {
    expect(urlencode('hello world')).toBe('hello%20world');
  });

  test('special characters', () => {
    const encoded = urlencode('hello#world&foo?bar=1');
    expect(encoded).toBe('hello%23world%26foo%3Fbar%3D1');
  });

  test('slash', () => {
    const encoded = urlencode('user/723666c1-b756-4b81');
    expect(encoded).toContain('%2F');
  });

  test('chinese characters', () => {
    const encoded = urlencode('你好');
    expect(encoded).toMatch(/^%[0-9A-F]{2}%[0-9A-F]{2}%[0-9A-F]{2}%[0-9A-F]{2}%[0-9A-F]{2}%[0-9A-F]{2}$/);
  });

  test('empty string', () => {
    expect(urlencode('')).toBe('');
  });

  test('percent sign itself', () => {
    expect(urlencode('100%')).toBe('100%25');
  });
});
