/**
 * URL-encode a string for query parameters (RFC 3986 percent-encoding).
 * Encodes everything except: A-Z a-z 0-9 - _ . ~
 * Uses UTF-8 byte encoding for non-ASCII characters.
 */
export function urlencode(s: string): string {
  const encoder = new TextEncoder();
  const bytes = encoder.encode(s);
  let result = '';
  for (const b of bytes) {
    if (
      (b >= 0x41 && b <= 0x5a) || // A-Z
      (b >= 0x61 && b <= 0x7a) || // a-z
      (b >= 0x30 && b <= 0x39) || // 0-9
      b === 0x2d || // -
      b === 0x5f || // _
      b === 0x2e || // .
      b === 0x7e    // ~
    ) {
      result += String.fromCharCode(b);
    } else {
      result += '%' + b.toString(16).toUpperCase().padStart(2, '0');
    }
  }
  return result;
}
