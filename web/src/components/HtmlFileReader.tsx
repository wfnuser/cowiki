import { useEffect, useState } from 'react';
import { bundleHtmlFile, type HtmlAssetReader } from '../lib/html-file';
import { sandboxedHtmlDocument } from '../lib/html-view';
import { C, fonts } from '../lib/design';

export function HtmlFileReader({ source, path, readAsset }: { source: string; path: string; readAsset: HtmlAssetReader }) {
  const [result, setResult] = useState<{ html: string; warnings: string[] } | null>(null);
  const [error, setError] = useState('');
  const [showSource, setShowSource] = useState(false);
  useEffect(() => {
    let active = true;
    bundleHtmlFile(source, path, readAsset).then(value => { if (active) setResult(value); }).catch(cause => { if (active) setError(String(cause)); });
    return () => { active = false; };
  }, [source, path, readAsset]);
  return <section style={{ position: 'absolute', inset: 0, display: 'flex', flexDirection: 'column', background: C.panel }}>
    <header style={{ display: 'flex', alignItems: 'center', gap: 12, padding: '10px 20px', borderBottom: `1px solid ${C.line}`, fontSize: 12, color: C.muted }}>
      <span style={{ flex: 1 }}>HTML · Read-only · Local resources only</span>
      <button type="button" onClick={() => setShowSource(!showSource)} style={{ cursor: 'pointer', border: 0, background: 'none', color: C.accent }}>{showSource ? 'Preview' : 'Source'}</button>
    </header>
    {error && <p role="alert">{error}</p>}
    {result && result.warnings.length > 0 && <details style={{ padding: '6px 20px', fontSize: 12, color: C.muted, maxHeight: 140, overflow: 'auto', flexShrink: 0 }}><summary>{result.warnings.length} resources could not be loaded</summary>{result.warnings.map(w => <p key={w}>{w}</p>)}</details>}
    {showSource ? <pre style={{ flex: 1, margin: 0, padding: 20, overflow: 'auto', fontFamily: fonts.mono, fontSize: 12 }}>{source}</pre>
      : result ? <iframe title="Local HTML document" sandbox="allow-scripts" referrerPolicy="no-referrer" srcDoc={sandboxedHtmlDocument(result.html)} style={{ flex: 1, minHeight: 0, width: '100%', border: 0 }} />
      : !error && <p role="status" style={{ padding: 20 }}>Loading local HTML resources…</p>}
  </section>;
}
