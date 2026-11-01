import assert from 'node:assert/strict';
import test from 'node:test';

import { reviewRoute } from '../src/components/review/review-navigation.ts';
import { showsRoute, viewForRoute } from '../src/lib/route-view.ts';

const spaces = ['general', 'thevault'];

test('review list -> detail -> back -> forward follows the URL', () => {
  const list = reviewRoute('qinghao', 'general');
  const detail = reviewRoute('qinghao', 'general', { kind: 'local-agent', changeId: 'c1' });
  const history = [list, detail, list, detail];
  let shown: Parameters<typeof showsRoute>[0] = null;
  const seen: string[] = [];
  for (const pathname of history) {
    const route = viewForRoute(pathname, spaces);
    assert.ok(route);
    if (!showsRoute(shown, 'general', route)) {
      shown = route;
      seen.push(route.kind);
    }
  }
  assert.deepEqual(seen, ['review-list', 'review-detail', 'review-list', 'review-detail']);
  assert.deepEqual(viewForRoute(detail, spaces), {
    kind: 'review-detail', workspaceSlug: 'general', target: { kind: 'local-agent', changeId: 'c1' },
  });
});

test('page URLs map to pages, and an already shown page needs no reload', () => {
  const route = viewForRoute('/thevault/Projects/CoWiki%20Roadmap', spaces);
  assert.deepEqual(route, { kind: 'page', workspaceSlug: 'thevault', conceptId: 'Projects/CoWiki Roadmap' });
  assert.equal(showsRoute({ kind: 'page', slug: 'Projects/CoWiki Roadmap' }, 'thevault', route!), true);
  assert.equal(showsRoute({ kind: 'page', slug: 'Other' }, 'thevault', route!), false);
  assert.equal(showsRoute({ kind: 'tasks' }, 'thevault', route!), false);
});

test('home, unknown Spaces and broken URLs stand for no view', () => {
  assert.equal(viewForRoute('/', spaces), null);
  assert.equal(viewForRoute('/thevault', spaces), null);
  assert.equal(viewForRoute('/elsewhere/Page', spaces), null);
  assert.equal(viewForRoute('/thevault/%E0%A4%A', spaces), null);
});
