import React, { useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { PageReader } from '../../src/components/PageReader';
import { CommentsProvider, CommentsPanel, CommentsHeaderToggle, commentMarkdownComponents } from '../../src/components/PageCommentsLayer';
import type { PageCommentStore } from '../../src/lib/page-comment-store';
import '../../src/index.css';

const store: PageCommentStore = {
  key: 'lineage-fixture', scope: 'local', scopeLabel: 'Local', currentUserId: 'reader', currentUserName: 'Reader',
  list: async () => ({ comments: [], snapshots: [] }), listMembers: async () => [],
  create: async () => { throw new Error('Read-only fixture'); },
  setResolved: async () => undefined, delete: async () => undefined,
};
const evidence = { sources: ['.cowiki/sources/interview.md'], agents: [{ name: 'Codex', changeId: 'change-1', task: 'Organize the team discussion into portable knowledge.' }], commit: { oid: '0123456789abcdef', author: 'Qinghao', summary: 'Document local-first collaboration', committedAt: 1789182846 }, review: { id: 'review-1', number: 12, title: 'Knowledge ownership and collaboration' } };
function Fixture() {
  const articleRef = useRef<HTMLElement>(null);
  const [other, setOther] = useState(false);
  const [opened, setOpened] = useState('');
  return <div style={{ height: '100dvh', display: 'flex', flexDirection: 'column' }}>
    <header style={{ height: 60, flexShrink: 0, display: 'flex', gap: 20, alignItems: 'center', padding: '0 24px', borderBottom: '1px solid var(--color-border)' }}>
      <span>CoWiki · Demo</span><button onClick={() => setOther(!other)}>Switch document</button><output>{opened}</output>
    </header>
    <CommentsProvider store={store} pageSlug={other ? 'other.md' : 'index.md'} source="# Local-first collaboration" articleRef={articleRef}>
      <div style={{ position: 'relative', flex: 1, minHeight: 0 }}>
        <PageReader key={String(other)} articleRef={articleRef} markdownComponents={commentMarkdownComponents}
          toolbar={<CommentsHeaderToggle />}
          body={other ? '# Another document' : '# Local-first collaboration\n\nKnowledge stays with the people who create it. A Space is an ordinary folder of Markdown files, with Git recording its changes.\n\n## Keep the reading experience focused\n\nSources and records stay out of the way until the reader needs to verify a claim.\n\n## Review before sharing\n\nAgents organize material. People choose what belongs in shared knowledge.\n\n```html\n<button onclick="this.textContent=\'Interactive success\'">Try HTML</button>\n```'}
          lineage={other ? { sources: [], agents: [], commit: null, review: null } : evidence}
          loadSource={async () => ({ title: 'Team discussion: knowledge ownership', content: '---\nsource_url: https://example.com/notes\ncaptured_at: 2026-09-12\ncontent_hash: test-hash\n---\n# Team discussion\n\nRetain local ownership while sharing selected changes.' })}
          onOpenSource={path => setOpened(path)} onOpenReview={id => setOpened(id)} aside={<CommentsPanel />} />
      </div>
    </CommentsProvider>
  </div>;
}
createRoot(document.getElementById('root')!).render(<Fixture />);
