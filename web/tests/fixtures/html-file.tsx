import React from 'react';
import { createRoot } from 'react-dom/client';
import { HtmlFileReader } from '../../src/components/HtmlFileReader';
import '../../src/index.css';
const assets: Record<string, string> = {
  'paper/style.css': 'h1{color:rgb(220, 10, 20)}',
  'paper/app.js': 'document.querySelector("button").onclick=()=>document.querySelector("output").textContent="Working";',
};
const readAsset = async (path: string) => {
  if (!(path in assets)) throw Error('Missing resource');
  return Array.from(new TextEncoder().encode(assets[path]));
};
createRoot(document.getElementById('root')!).render(<HtmlFileReader path="paper/index.html" readAsset={readAsset}
  source={'<!doctype html><html><head><link rel="stylesheet" href="style.css"><script defer src="app.js"></script></head><body class="presentation"><h1>Real HTML file</h1><button>Run demo</button><output></output><a href="#end">Jump</a><div style="height:1400px"></div><p id="end">Jump target</p><img src="http://127.0.0.1:1/tracking"></body></html>'} />);
