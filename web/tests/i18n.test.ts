import assert from 'node:assert/strict';
import test from 'node:test';

import {
  intlLocale,
  loadLocale,
  LOCALE_STORAGE_KEY,
  saveLocale,
  translate,
  translateLabel,
  translateGovernanceDiagnostic,
  ZH_CN,
} from '../src/lib/i18n.ts';

test('locale defaults to English and persists per device', () => {
  const values = new Map<string, string>();
  const storage = {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => { values.set(key, value); },
  };
  assert.equal(loadLocale(storage), 'en');
  saveLocale('zh-CN', storage);
  assert.equal(values.get(LOCALE_STORAGE_KEY), 'zh-CN');
  assert.equal(loadLocale(storage), 'zh-CN');

  values.set(LOCALE_STORAGE_KEY, 'fr');
  assert.equal(loadLocale(storage), 'en');

  const broken = {
    getItem: () => { throw new Error('blocked'); },
    setItem: () => { throw new Error('blocked'); },
  };
  assert.equal(loadLocale(broken), 'en');
  assert.doesNotThrow(() => saveLocale('zh-CN', broken));
});

test('translations support interpolation, fallback labels, and non-empty Chinese copy', () => {
  assert.equal(translate('en', 'Settings'), 'Settings');
  assert.equal(translate('zh-CN', 'Settings'), '设置');
  assert.equal(translate('en', '{count} cards to review today.', { count: 12 }), '12 cards to review today.');
  assert.equal(translate('zh-CN', '{count} cards to review today.', { count: 12 }), '今天有 12 张卡要复习。');
  assert.equal(translateLabel('zh-CN', 'Daily journal'), '每日日记');
  assert.equal(translateLabel('zh-CN', 'Custom User Title'), 'Custom User Title');
  assert.equal(intlLocale('en'), 'en-US');
  assert.equal(intlLocale('zh-CN'), 'zh-CN');

  for (const [key, value] of Object.entries(ZH_CN)) {
    assert.ok(value.trim().length > 0, `Empty zh-CN translation for "${key}"`);
  }
});

test('native governance diagnostics preserve source values and unknown errors', () => {
  const raw = "Invalid trust_tier 'external-{tier}'; MUST be one of: human_verified, agent_draft.";
  assert.equal(translateGovernanceDiagnostic('en', raw), raw);
  assert.equal(translateGovernanceDiagnostic('zh-CN', raw),
    'trust_tier 值“external-{tier}”无效，可用值为 human_verified, agent_draft。');
  assert.equal(translateGovernanceDiagnostic('zh-CN', 'Unknown native error (42)'), 'Unknown native error (42)');
});
