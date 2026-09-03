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
  if (/<html(?:\s|>)/i.test(source)) {
    if (/<head(?:\s|>)/i.test(source)) {
      return source.replace(/<head([^>]*)>/i, `<head$1>${POLICY_META}`);
    }
    return source.replace(/<html([^>]*)>/i, `<html$1><head>${POLICY_META}</head>`);
  }
  return `<!doctype html>
<html>
<head>
${POLICY_META}
<style>
  :root { color-scheme: light dark; font-family: Inter, ui-sans-serif, system-ui, sans-serif; }
  body { margin: 0; padding: 24px; }
</style>
</head>
<body>${source}</body>
</html>`;
}
