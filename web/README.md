# CoWiki Web/Desktop Client

React + TypeScript + Vite client for CoWiki, with a Tauri 2 desktop shell.

## Development

```bash
npm install
npm run dev
```

Vite proxies `/api` to `http://localhost:3000`.

## Desktop

```bash
npm run desktop:dev
```

In desktop mode, the app defaults to `http://localhost:3000/api` because there is no browser-origin `/api` proxy inside the packaged app.

Override the API origin with either:

```bash
VITE_API_BASE=https://api-test.cowiki.app npm run desktop:dev
```

or at runtime:

```js
localStorage.setItem('cowiki.apiOrigin', 'https://api-test.cowiki.app')
```

## Build

```bash
npm run build
npm run desktop:build
```
