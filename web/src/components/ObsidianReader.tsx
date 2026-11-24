import {
  createContext,
  useContext,
  useEffect,
  useState,
  type AnchorHTMLAttributes,
  type HTMLAttributes,
  type ImgHTMLAttributes,
  type InputHTMLAttributes,
  type ReactNode,
} from 'react';
import type { Components } from 'react-markdown';
import { CALLOUT_SEP, calloutOf } from '@/lib/callouts';
import { C } from '@/lib/design';
import { wikilinkFromHref } from '@/lib/obsidian-markdown';
import { openExternalUrl } from '@/external-links';

export interface ReaderCallbacks {
  /** Navigate to a wikilink target (raw `[[…]]` inner text). */
  onWikilink?: (target: string) => void;
  /** Resolve a local attachment reference to a displayable URL. */
  loadAsset?: (src: string) => Promise<string>;
  /** Toggle the task on a 1-based line of the rendered Markdown. */
  onToggleTask?: (renderedLine: number) => void;
}

const REMOTE = /^(https?:|data:|blob:)/i;

function AssetImage({ src, alt, loadAsset, ...rest }: ImgHTMLAttributes<HTMLImageElement> & { loadAsset?: (src: string) => Promise<string> }) {
  const raw = typeof src === 'string' ? src : '';
  const remote = REMOTE.test(raw);
  const [resolved, setResolved] = useState<{ src: string; url: string | null; error: string | null } | null>(null);

  useEffect(() => {
    if (remote || !raw || !loadAsset) return;
    let cancelled = false;
    loadAsset(raw).then(
      (url) => { if (!cancelled) setResolved({ src: raw, url, error: null }); },
      (error: unknown) => {
        if (!cancelled) {
          setResolved({ src: raw, url: null, error: error instanceof Error ? error.message : String(error) });
        }
      },
    );
    return () => { cancelled = true; };
  }, [raw, remote, loadAsset]);

  if (remote) return <img src={raw} alt={alt} loading="lazy" {...rest} />;
  const current = resolved?.src === raw ? resolved : null;
  if (current?.url) return <img src={current.url} alt={alt} loading="lazy" {...rest} />;
  return (
    <span
      role="img"
      aria-label={alt || raw}
      title={current?.error ?? raw}
      style={{
        display: 'inline-flex', alignItems: 'center', gap: 6, padding: '6px 10px', borderRadius: 6,
        border: `1px dashed ${C.line}`, color: current?.error ? C.red : C.muted, fontSize: 13,
      }}
    >
      {current?.error ? `Attachment not found: ${decodeURIComponent(raw)}` : 'Loading attachment…'}
    </span>
  );
}

function WikiAnchor({ href, children, onWikilink, ...rest }: AnchorHTMLAttributes<HTMLAnchorElement> & { onWikilink?: (target: string) => void }) {
  const target = wikilinkFromHref(href);
  if (target !== null) {
    return (
      <a
        {...rest}
        href={href}
        className="wikilink"
        data-wikilink={target}
        onClick={(event) => {
          event.preventDefault();
          onWikilink?.(target);
        }}
      >
        {children}
      </a>
    );
  }
  if (href && /^https?:\/\//i.test(href)) {
    return (
      <a
        {...rest}
        href={href}
        onClick={(event) => {
          event.preventDefault();
          void openExternalUrl(href);
        }}
      >
        {children}
      </a>
    );
  }
  return <a {...rest} href={href}>{children}</a>;
}

function TaskCheckbox({ checked, onToggleTask, ...rest }: InputHTMLAttributes<HTMLInputElement> & { onToggleTask?: (line: number) => void }) {
  if (rest.type !== 'checkbox') return <input checked={checked} {...rest} />;
  return (
    <input
      type="checkbox"
      checked={!!checked}
      disabled={!onToggleTask}
      className="reader-task-checkbox"
      style={{ cursor: onToggleTask ? 'pointer' : undefined }}
      onChange={(event) => {
        const item = event.currentTarget.closest('li[data-source-line]') as HTMLElement | null;
        const line = Number(item?.dataset.sourceLine);
        if (Number.isFinite(line) && line > 0) onToggleTask?.(line);
      }}
    />
  );
}

/** Callbacks for the Obsidian reader components; provide them around the reader. */
export const ObsidianReaderContext = createContext<ReaderCallbacks>({});

type WithNode<T> = T & { node?: unknown };

function ReaderAnchor({ node, ...props }: WithNode<AnchorHTMLAttributes<HTMLAnchorElement>>) {
  void node;
  const { onWikilink } = useContext(ObsidianReaderContext);
  return <WikiAnchor {...props} onWikilink={onWikilink} />;
}

function ReaderImage({ node, ...props }: WithNode<ImgHTMLAttributes<HTMLImageElement>>) {
  void node;
  const { loadAsset } = useContext(ObsidianReaderContext);
  return <AssetImage {...props} loadAsset={loadAsset} />;
}

function ReaderInput({ node, ...props }: WithNode<InputHTMLAttributes<HTMLInputElement>>) {
  void node;
  const { onToggleTask } = useContext(ObsidianReaderContext);
  return <TaskCheckbox {...props} onToggleTask={onToggleTask} />;
}

/** A callout's title (`**<SEP>tip<SEP> Title**`, see lib/callouts.ts): its icon, then the title. */
function ReaderStrong({ node, children, ...props }: WithNode<HTMLAttributes<HTMLElement>>) {
  void node;
  const parts = Array.isArray(children) ? children : [children];
  const first = parts[0];
  if (typeof first === 'string' && first.startsWith(CALLOUT_SEP)) {
    const end = first.indexOf(CALLOUT_SEP, 1);
    const { icon, family } = calloutOf(first.slice(1, end));
    const rest: ReactNode[] = [first.slice(end + 1).trimStart(), ...parts.slice(1)];
    return (
      <strong className="callout-title" data-callout-family={family}>
        <span aria-hidden className="callout-icon" data-icon={icon} />
        {rest}
      </strong>
    );
  }
  return <strong {...props}>{children}</strong>;
}

/**
 * Markdown components that make Obsidian-style pages usable in the reader.
 * Wrap the reader in `ObsidianReaderContext.Provider` to enable navigation,
 * attachments and task toggles.
 */
export function withObsidianReader(base: Components | undefined): Components {
  return {
    ...(base ?? {}),
    a: ReaderAnchor,
    img: ReaderImage,
    input: ReaderInput,
    strong: ReaderStrong,
  };
}
