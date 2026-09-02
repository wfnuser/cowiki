import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { FileText, Search, Sparkles, CornerDownLeft, X } from 'lucide-react';
import { searchWorkspace, type SearchResponse } from '../api';
import { Dialog, DialogContent, DialogTitle } from '@/components/ui/dialog';
import { C, fonts } from '@/lib/design';
import { isDesktopClient } from '@/runtime';
import { conceptIdFromPath, conceptPath, isSearchableConceptPath } from '@/lib/okf-pages';

/** Highlight occurrences of `q` inside `text` (case-insensitive). */
function Highlight({ text, q }: { text: string; q: string }) {
  if (!q.trim()) return <>{text}</>;
  const lower = text.toLowerCase();
  const needle = q.trim().toLowerCase();
  const parts: React.ReactNode[] = [];
  let i = 0;
  let k = 0;
  while (i < text.length) {
    const at = lower.indexOf(needle, i);
    if (at === -1) {
      parts.push(text.slice(i));
      break;
    }
    if (at > i) parts.push(text.slice(i, at));
    parts.push(
      <mark
        key={k++}
        style={{ background: C.accentSoft, color: C.accent, borderRadius: 3, padding: '0 1px' }}
      >
        {text.slice(at, at + needle.length)}
      </mark>
    );
    i = at + needle.length;
  }
  return <>{parts}</>;
}

type Row =
  | { kind: 'keyword'; slug: string; path?: string; title: string; snippet: string; titleMatch: boolean }
  | { kind: 'semantic'; slug: string; title: string; summary: string; source: string };

/**
 * Feishu-style search palette: centered modal over a dimmed canvas.
 * Keyword (full-text) hits with highlighted snippets, then a Semantic section.
 * ↑↓ to move, Enter to open, Esc to close.
 */
export function SearchModal({
  workspaceSlug,
  workspaceName,
  open,
  onClose,
  onSelectPage,
}: {
  workspaceSlug: string;
  workspaceName: string;
  open: boolean;
  onClose: () => void;
  onSelectPage: (slug: string, path?: string) => void;
}) {
  const [q, setQ] = useState('');
  const [kw, setKw] = useState<SearchResponse['keyword'] | null>(null);
  const [sem, setSem] = useState<SearchResponse['semantic'] | null>(null);
  const [kwLoading, setKwLoading] = useState(false);
  const [semLoading, setSemLoading] = useState(false);
  const [sel, setSel] = useState(0);
  const kwTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const semTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  // Reset on open. Focus/Esc/outside-click are handled by the Dialog primitive:
  // it auto-focuses the first focusable element (the search input) and closes on
  // Escape no matter where focus is, plus on pointer-down outside the panel.
  useEffect(() => {
    if (open) {
      setQ('');
      setKw(null);
      setSem(null);
      setSel(0);
    }
  }, [open]);

  // Two parallel debounced fetches: keyword is a fast local scan (short debounce,
  // renders first); semantic needs an embedding round-trip (longer debounce, fills
  // in below when ready) — same pattern as the old inline sidebar search.
  useEffect(() => {
    if (!open) return;
    clearTimeout(kwTimer.current);
    clearTimeout(semTimer.current);
    if (!q.trim()) {
      setKw(null);
      setSem(null);
      setKwLoading(false);
      setSemLoading(false);
      return;
    }
    setKwLoading(true);
    kwTimer.current = setTimeout(() => {
      searchWorkspace(workspaceSlug, q.trim(), 'keyword')
        .then((r) => { setKw(r.keyword); setSel(0); })
        .catch(() => setKw([]))
        .finally(() => setKwLoading(false));
    }, 120);
    if (isDesktopClient()) {
      // The local index is intentionally lexical in V1. Do not scan the same
      // repository twice for an empty semantic result.
      setSem([]);
      setSemLoading(false);
    } else {
      setSemLoading(true);
      semTimer.current = setTimeout(() => {
        searchWorkspace(workspaceSlug, q.trim(), 'semantic')
          .then((r) => setSem(r.semantic))
          .catch(() => setSem([]))
          .finally(() => setSemLoading(false));
      }, 300);
    }
    return () => { clearTimeout(kwTimer.current); clearTimeout(semTimer.current); };
  }, [q, open, workspaceSlug]);

  // Flattened rows for keyboard navigation (semantic deduped against keyword).
  const rows: Row[] = useMemo(() => {
    const keywordHits = (kw ?? []).filter((hit) =>
      isSearchableConceptPath(conceptPath(hit.slug)));
    const out: Row[] = keywordHits.map((h) => {
      const slug = conceptIdFromPath(h.slug);
      return {
        kind: 'keyword', slug, path: conceptPath(slug), title: h.title, snippet: h.snippet, titleMatch: h.title_match,
      };
    });
    const seen = new Set(out.map((hit) => hit.slug));
    for (const h of sem ?? []) {
      const slug = conceptIdFromPath(h.slug);
      if (!seen.has(slug)) {
        out.push({ kind: 'semantic', slug, title: h.title, summary: h.summary, source: h.source });
      }
    }
    return out;
  }, [kw, sem]);
  const semStart = rows.findIndex((r) => r.kind === 'semantic');

  const pick = useCallback((row: Row | undefined) => {
    if (!row) return;
    onClose();
    onSelectPage(row.slug, row.kind === 'keyword' ? row.path : undefined);
  }, [onClose, onSelectPage]);

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'ArrowDown') { e.preventDefault(); setSel((s) => Math.min(s + 1, rows.length - 1)); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); setSel((s) => Math.max(s - 1, 0)); }
    else if (e.key === 'Enter') { e.preventDefault(); pick(rows[sel]); }
  };

  // Keep the selected row in view.
  useEffect(() => {
    listRef.current
      ?.querySelector(`[data-row="${sel}"]`)
      ?.scrollIntoView({ block: 'nearest' });
  }, [sel]);

  const sectionLabel = (icon: React.ReactNode, label: string) => (
    <div style={{
      display: 'flex', alignItems: 'center', gap: 6, padding: '10px 18px 4px',
      fontSize: 11, fontWeight: 600, color: C.faint, textTransform: 'uppercase', letterSpacing: '0.07em',
    }}>
      {icon} {label}
    </div>
  );

  return (
    <Dialog open={open} onOpenChange={(next) => { if (!next) onClose(); }}>
      {/* ui/Dialog already carries this palette's scrim, 16vh anchor, 14px
          radius, border and layered shadow; strip its padding/gap/close button
          and open animation so the palette keeps its exact previous layout. */}
      <DialogContent
        showCloseButton={false}
        aria-label="Search"
        className="flex max-h-[60vh] w-[min(620px,92vw)] max-w-[min(620px,92vw)] flex-col gap-0 overflow-hidden p-0 data-[state=closed]:animate-none data-[state=open]:animate-none sm:max-w-[min(620px,92vw)]"
      >
        <DialogTitle className="sr-only">Search</DialogTitle>
        {/* Input row */}
        <div style={{
          display: 'flex', alignItems: 'center', gap: 10, padding: '14px 18px',
          borderBottom: `1px solid ${C.lineSoft}`,
        }}>
          <Search size={17} color={C.muted} style={{ flexShrink: 0 }} />
          <input
            ref={inputRef}
            value={q}
            onChange={(e) => setQ(e.target.value)}
            onKeyDown={onKeyDown}
            placeholder={`Search in ${workspaceName}…`}
            style={{
              flex: 1, minWidth: 0, background: 'transparent', border: 'none', outline: 'none',
              fontSize: 15.5, color: C.ink, fontFamily: 'inherit',
            }}
          />
          {q && (
            <button
              onClick={() => { setQ(''); inputRef.current?.focus(); }}
              aria-label="Clear search"
              style={{
                display: 'flex', alignItems: 'center', justifyContent: 'center',
                width: 22, height: 22, borderRadius: 6, flexShrink: 0,
                background: 'transparent', border: 'none', cursor: 'pointer', color: C.faint,
              }}
              onMouseEnter={(e) => { e.currentTarget.style.background = C.rail; e.currentTarget.style.color = C.muted; }}
              onMouseLeave={(e) => { e.currentTarget.style.background = 'transparent'; e.currentTarget.style.color = C.faint; }}
            >
              <X size={15} />
            </button>
          )}
        </div>

        {/* Results — fixed minimum height so the panel never collapses to a sliver */}
        <div ref={listRef} style={{ flex: 1, minHeight: 200, overflowY: 'auto', paddingBottom: 6 }}>
          {!q.trim() ? (
            <Empty>Search pages by title or content in this space</Empty>
          ) : rows.length === 0 && (kwLoading || semLoading) ? (
            <Empty>Searching…</Empty>
          ) : rows.length === 0 ? (
            <Empty>No results for “{q}”</Empty>
          ) : (
            <>
              {rows.length > 0 && rows[0].kind === 'keyword' &&
                sectionLabel(<FileText size={11} />, 'Pages')}
              {rows.map((row, i) => (
                <div key={`${row.kind}-${row.slug}`}>
                  {i === semStart && sectionLabel(<Sparkles size={11} />, 'Semantic')}
                  <div
                    data-row={i}
                    onMouseEnter={() => setSel(i)}
                    onMouseDown={(e) => { e.preventDefault(); pick(row); }}
                    style={{
                      display: 'flex', alignItems: 'flex-start', gap: 12,
                      margin: '0 8px', padding: '9px 10px', borderRadius: 9, cursor: 'pointer',
                      background: sel === i ? C.accentSoft : 'transparent',
                      transition: 'background 0.06s',
                    }}
                  >
                    <div style={{
                      width: 30, height: 30, borderRadius: 8, flexShrink: 0, marginTop: 1,
                      background: sel === i ? C.accent : C.rail,
                      color: sel === i ? C.onAccent : C.muted,
                      display: 'flex', alignItems: 'center', justifyContent: 'center',
                    }}>
                      {row.kind === 'semantic' ? <Sparkles size={14} /> : <FileText size={14} />}
                    </div>
                    <div style={{ flex: 1, minWidth: 0 }}>
                      <div style={{
                        fontSize: 14, fontWeight: 550, color: C.ink, lineHeight: 1.35,
                        overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
                        display: 'flex', alignItems: 'center', gap: 6,
                      }}>
                        <span style={{ overflow: 'hidden', textOverflow: 'ellipsis' }}>
                          <Highlight text={row.title} q={q} />
                        </span>
                        {row.kind === 'semantic' && row.source === 'draft' && (
                          <span style={{
                            fontSize: 10, fontWeight: 600, padding: '1px 6px', borderRadius: 999,
                            background: C.accentSoft, color: C.accent, flexShrink: 0,
                            border: `1px solid ${sel === i ? C.accent : 'transparent'}`,
                          }}>
                            draft
                          </span>
                        )}
                      </div>
                      <div style={{
                        fontSize: 12.5, color: C.muted, marginTop: 2, lineHeight: 1.4,
                        overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
                      }}>
                        {row.kind === 'keyword'
                          ? (row.titleMatch
                              ? <span style={{ fontFamily: fonts.mono, fontSize: 11.5, color: C.faint }}>{row.slug}</span>
                              : <Highlight text={row.snippet} q={q} />)
                          : (row.summary || <span style={{ fontFamily: fonts.mono, fontSize: 11.5, color: C.faint }}>{row.slug}</span>)}
                      </div>
                    </div>
                    {sel === i && (
                      <CornerDownLeft size={14} color={C.faint} style={{ flexShrink: 0, alignSelf: 'center' }} />
                    )}
                  </div>
                </div>
              ))}
              {semLoading && semStart === -1 && (
                <>
                  {sectionLabel(<Sparkles size={11} />, 'Semantic')}
                  <div style={{ padding: '4px 18px 8px', fontSize: 12.5, color: C.faint }}>Searching…</div>
                </>
              )}
            </>
          )}
        </div>

        {/* Footer hints */}
        <div style={{
          display: 'flex', alignItems: 'center', gap: 14, padding: '8px 18px',
          borderTop: `1px solid ${C.lineSoft}`, background: C.bg,
          fontSize: 11.5, color: C.faint,
        }}>
          <span><Kbd>↑</Kbd><Kbd>↓</Kbd> select</span>
          <span><Kbd>↵</Kbd> open</span>
          <span><Kbd>esc</Kbd> close</span>
          <span style={{ marginLeft: 'auto' }}>Searches your view: drafts + published</span>
        </div>
      </DialogContent>
    </Dialog>
  );
}

/** Vertically-centered placeholder for the fixed-height results area. */
function Empty({ children }: { children: React.ReactNode }) {
  return (
    <div style={{
      height: '100%', minHeight: 200, display: 'flex', alignItems: 'center', justifyContent: 'center',
      color: C.faint, fontSize: 13.5, padding: '0 18px', textAlign: 'center',
    }}>
      {children}
    </div>
  );
}

function Kbd({ children }: { children: React.ReactNode }) {
  return (
    <kbd style={{
      display: 'inline-flex', alignItems: 'center', justifyContent: 'center',
      minWidth: 16, height: 16, padding: '0 4px', marginRight: 3,
      background: C.panel, border: `1px solid ${C.line}`, borderBottomWidth: 2,
      borderRadius: 4, fontSize: 10.5, fontFamily: 'inherit', color: C.muted,
    }}>
      {children}
    </kbd>
  );
}

export default SearchModal;
