import { parse, type DefaultTreeAdapterMap } from 'parse5';

export type HtmlAssetReader = (path: string) => Promise<number[]>;
const MIME: Record<string, string> = { png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg', gif: 'image/gif', webp: 'image/webp', svg: 'image/svg+xml', ico: 'image/x-icon', woff: 'font/woff', woff2: 'font/woff2', ttf: 'font/ttf', otf: 'font/otf', mp3: 'audio/mpeg', wav: 'audio/wav', mp4: 'video/mp4', webm: 'video/webm' };

/** Package resources in memory. The inert template is never attached to the host DOM. */
export async function bundleHtmlFile(source: string, path: string, read: HtmlAssetReader): Promise<{ html: string; warnings: string[] }> {
  const warnings: string[] = [];
  // Parse document attributes without creating a live browsing context or fetching resources.
  const root = parse(source).childNodes.find(node => node.nodeName === 'html') as DefaultTreeAdapterMap['element'];
  const body = root.childNodes.find(node => node.nodeName === 'body') as DefaultTreeAdapterMap['element'];
  const template = document.createElement('template');
  template.innerHTML = source;
  const base = new URL(path, 'https://html.invalid/');
  const directory = new URL('.', base).pathname;
  let bytesRead = 0;
  const cache = new Map<string, Promise<number[]>>();
  function resolve(value: string, from: string) {
    if (/^(?:[a-z][a-z\d+.-]*:|\/\/|\/)/i.test(value.trim())) throw Error('Only relative local resources are supported');
    const url = new URL(value, new URL(from, base.origin + '/'));
    const decoded = decodeURIComponent(url.pathname);
    if (!url.pathname.startsWith(directory) || decoded.split('/').some(part => part.startsWith('.') || part.includes('\\'))) throw Error('Resource escapes the HTML directory');
    return decoded.slice(1);
  }
  async function load(value: string, from: string) {
    const resolved = resolve(value, from);
    if (!cache.has(resolved)) {
      if (cache.size >= 160) throw Error('Too many HTML resources');
      cache.set(resolved, read(resolved).then(bytes => {
        bytesRead += bytes.length;
        if (bytes.length > 8 * 1024 * 1024 || bytesRead > 24 * 1024 * 1024) throw Error('HTML resource size limit exceeded');
        return bytes;
      }));
    }
    return { path: resolved, bytes: await cache.get(resolved)! };
  }
  const text = (bytes: number[]) => new TextDecoder().decode(new Uint8Array(bytes));
  async function dataUrl(value: string, from: string): Promise<string> {
    if (/^data:(?:image|font|audio|video)\//i.test(value)) return value;
    const asset = await load(value, from);
    const mime = MIME[asset.path.split('.').at(-1)!.toLowerCase()];
    if (!mime) throw Error('Unsupported media type');
    let binary = '';
    for (let i = 0; i < asset.bytes.length; i += 8192) binary += String.fromCharCode(...asset.bytes.slice(i, i + 8192));
    return `data:${mime};base64,${btoa(binary)}`;
  }
  async function css(content: string, from: string) {
    // CSS imports require a graph resolver; do not silently enable network fallback.
    content = content.replace(/@import\s+[^;]+;/gi, () => { warnings.push('CSS @import is not supported; use a local stylesheet link.'); return ''; });
    // Font source lists are alternatives, not a requirement to ship every legacy format.
    for (const face of [...content.matchAll(/@font-face\s*\{[^}]*\}/gi)]) {
      const src = face[0].match(/\bsrc\s*:\s*((?:url\([^)]*\)|format\([^)]*\)|[^;}])*)/i);
      if (!src) continue;
      let embedded = '';
      for (const candidate of src[1].matchAll(/url\(\s*(['"]?)(.*?)\1\s*\)/gi)) {
        try { embedded = await dataUrl(candidate[2], from); break; } catch { /* Try the next font format. */ }
      }
      if (!embedded) warnings.push('No local font source available in ' + from);
      content = content.replace(face[0], face[0].replace(src[0], `src:url("${embedded}")`));
    }
    const matches = [...content.matchAll(/url\(\s*(['"]?)(.*?)\1\s*\)/gi)];
    for (const match of matches) {
      if (!match[2].trim() || match[2].startsWith('#')) continue;
      try { content = content.replace(match[0], `url("${await dataUrl(match[2], from)}")`); }
      catch { warnings.push(`Resource unavailable: ${match[2]}`); content = content.replace(match[0], 'url("")'); }
    }
    return content;
  }
  template.content.querySelectorAll('base, meta[http-equiv], iframe, object, embed').forEach(node => node.remove());
  for (const link of template.content.querySelectorAll('link')) {
    if (link.getAttribute('rel') !== 'stylesheet') { link.remove(); continue; }
    try {
      const asset = await load(link.getAttribute('href') || '', path);
      const style = document.createElement('style');
      style.textContent = await css(text(asset.bytes), asset.path);
      link.replaceWith(style);
    } catch { warnings.push(`Stylesheet unavailable: ${link.getAttribute('href')}`); link.remove(); }
  }
  for (const style of template.content.querySelectorAll('style')) style.textContent = await css(style.textContent || '', path);
  for (const element of template.content.querySelectorAll('[style]')) element.setAttribute('style', await css(element.getAttribute('style')!, path));
  for (const image of template.content.querySelectorAll('img, source, video, audio, image')) {
    image.removeAttribute('srcset');
    for (const attr of ['src', 'poster', 'href', 'xlink:href']) {
      if (!image.hasAttribute(attr)) continue;
      try { image.setAttribute(attr, await dataUrl(image.getAttribute(attr)!, path)); }
      catch { warnings.push(`Resource unavailable: ${image.getAttribute(attr)}`); image.removeAttribute(attr); }
    }
  }
  for (const script of [...template.content.querySelectorAll('script')]) {
    if (script.hasAttribute('src')) {
      try { script.textContent = text((await load(script.getAttribute('src')!, path)).bytes).replace(/<\/script/gi, '<\\/script'); }
      catch { warnings.push(`Script unavailable: ${script.getAttribute('src')}`); script.remove(); continue; }
      script.removeAttribute('src');
    }
    script.removeAttribute('defer'); script.removeAttribute('async');
    // Inline replacements must execute after the document exists, in source order.
    template.content.append(script);
  }
  const navigation = document.createElement('script');
  navigation.textContent = `document.addEventListener('click', function(event) {
    const link = event.target instanceof Element ? event.target.closest('a[href^="#"]') : null;
    if (!link) return;
    event.preventDefault();
    try { const target = document.getElementById(decodeURIComponent(link.getAttribute('href').slice(1))); if (target) target.scrollIntoView({block:'start'}); } catch {}
  });`;
  template.content.append(navigation);
  const escapeAttribute = (value: string) => value.replaceAll('&', '&amp;').replaceAll('"', '&quot;').replaceAll('<', '&lt;');
  async function attributes(element: DefaultTreeAdapterMap['element']) {
    const values = [];
    for (const attr of element.attrs) {
      const value = attr.name === 'style' ? await css(attr.value, path) : attr.value;
      values.push(` ${attr.name}="${escapeAttribute(value)}"`);
    }
    return values.join('');
  }
  return { html: `<!doctype html><html${await attributes(root)}><body${await attributes(body)}>${template.innerHTML}</body></html>`, warnings: [...new Set(warnings)] };
}
