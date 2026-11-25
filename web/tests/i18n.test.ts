import assert from 'node:assert/strict';
import test from 'node:test';

import {
  intlLocale,
  loadLocale,
  LOCALE_STORAGE_KEY,
  saveLocale,
  translate,
  translateLabel,
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
  assert.equal(translate('en', 'Study all · {count}', { count: 12 }), 'Study all · 12');
  assert.equal(translate('zh-CN', 'Study all · {count}', { count: 12 }), '全部学习 · 12');
  assert.equal(translateLabel('zh-CN', 'Daily journal'), '每日日记');
  assert.equal(translateLabel('zh-CN', 'Custom User Title'), 'Custom User Title');
  assert.equal(intlLocale('en'), 'en-US');
  assert.equal(intlLocale('zh-CN'), 'zh-CN');

  for (const [key, value] of Object.entries(ZH_CN)) {
    assert.ok(value.trim().length > 0, `Empty zh-CN translation for "${key}"`);
  }
});
