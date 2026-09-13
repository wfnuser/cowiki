const HTML_FENCE_PATTERN = /(?:^|\s)language-html(?:\s|$)/i;

const SANDBOX_CSP = [
  "default-src 'none'",
  "style-src 'unsafe-inline'",
  "script-src 'unsafe-inline'",
  'img-src data: blob:',
  'font-src data:',
  'media-src data: blob:',
  "connect-src 'none'",
  "form-action 'none'",
  "base-uri 'none'",
  "frame-src 'none'",
].join('; ');

const POLICY_META = `<meta http-equiv="Content-Security-Policy" content="${SANDBOX_CSP}">`;

export function isHtmlCodeLanguage(className: string | undefined): boolean {
  return HTML_FENCE_PATTERN.test(className ?? '');
}

/**
 * Wrap a Markdown HTML fence as an isolated, self-contained document. The
 * iframe sandbox is the primary boundary; this CSP also prevents the preview
 * from making network requests or loading remote assets.
 */
export function sandboxedHtmlDocument(source: string): string {
  // An immutable outer document owns the child frame's navigation policy.
  // A CSP inside untrusted HTML alone cannot prevent self-navigation, and
  // searching that HTML for <head> can inject the policy into a comment.
  // srcdoc is local (no fetch); frame-src 'none' blocks subsequent URL loads.
  const inner = `<!doctype html><html><head>${POLICY_META}<style>
    :root { color-scheme: light dark; font-family: Inter, ui-sans-serif, system-ui, sans-serif; }
    body { margin: 0; padding: 24px; }
  </style></head><body>${source}</body></html>`;
  const escaped = inner.replaceAll('&', '&amp;').replaceAll('"', '&quot;').replaceAll('<', '&lt;').replaceAll('>', '&gt;');
  return `<!doctype html>
<html>
<head>
${POLICY_META}
<style>
  html, body { margin: 0; width: 100%; height: 100%; overflow: hidden; }
  iframe { display: block; width: 100%; height: 100%; border: 0; }
</style>
</head>
<body><iframe title="HTML content" sandbox="allow-scripts" referrerpolicy="no-referrer" srcdoc="${escaped}"></iframe></body>
</html>`;
}
