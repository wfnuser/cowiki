import { describe, test, expect, vi, beforeEach } from 'vitest';
import { printTable, printSuccess, printError, printInfo, printJson, printWarning } from '../../src/output.js';

describe('output formatting', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  describe('printJson', () => {
    test('outputs formatted JSON', () => {
      const spy = vi.spyOn(console, 'log').mockImplementation(() => {});
      printJson({ slug: 'test', title: 'Hello' });
      const output = spy.mock.calls[0][0];
      const parsed = JSON.parse(output);
      expect(parsed).toEqual({ slug: 'test', title: 'Hello' });
    });

    test('outputs arrays', () => {
      const spy = vi.spyOn(console, 'log').mockImplementation(() => {});
      printJson([{ a: 1 }, { b: 2 }]);
      const parsed = JSON.parse(spy.mock.calls[0][0]);
      expect(parsed).toHaveLength(2);
    });

    test('outputs null', () => {
      const spy = vi.spyOn(console, 'log').mockImplementation(() => {});
      printJson(null);
      expect(spy.mock.calls[0][0]).toBe('null');
    });
  });

  describe('printTable', () => {
    test('outputs table with headers', () => {
      const spy = vi.spyOn(console, 'log').mockImplementation(() => {});
      printTable(['NAME', 'VALUE'], [['foo', 'bar']]);
      const output = spy.mock.calls[0][0];
      expect(output).toContain('NAME');
      expect(output).toContain('VALUE');
      expect(output).toContain('foo');
      expect(output).toContain('bar');
    });

    test('outputs empty table', () => {
      const spy = vi.spyOn(console, 'log').mockImplementation(() => {});
      printTable(['COL1'], []);
      expect(spy.mock.calls[0][0]).toBeDefined();
    });
  });

  describe('printSuccess', () => {
    test('prints green checkmark', () => {
      const spy = vi.spyOn(console, 'log').mockImplementation(() => {});
      printSuccess('Done');
      expect(spy.mock.calls[0][0]).toContain('Done');
      expect(spy.mock.calls[0][0]).toContain('✓');
    });
  });

  describe('printError', () => {
    test('prints red X to stderr', () => {
      const spy = vi.spyOn(console, 'error').mockImplementation(() => {});
      printError('Fail');
      expect(spy.mock.calls[0][0]).toContain('Fail');
      expect(spy.mock.calls[0][0]).toContain('✗');
    });
  });

  describe('printInfo', () => {
    test('prints cyan info', () => {
      const spy = vi.spyOn(console, 'log').mockImplementation(() => {});
      printInfo('Note');
      expect(spy.mock.calls[0][0]).toContain('Note');
      expect(spy.mock.calls[0][0]).toContain('ℹ');
    });
  });

  describe('printWarning', () => {
    test('prints yellow warning to stderr', () => {
      const spy = vi.spyOn(console, 'error').mockImplementation(() => {});
      printWarning('Careful');
      expect(spy.mock.calls[0][0]).toContain('Careful');
      expect(spy.mock.calls[0][0]).toContain('⚠');
    });
  });
});
