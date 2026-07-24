#!/usr/bin/env node
// Regenerate the language-specific app screenshots used by the setup-guide tab
// (server/src/web/components/tutorial.rs). Drives the running dev app over the
// Chrome DevTools Protocol — no extra npm deps.
//
// Prereqs:
//   • the dev app is running and reachable (default http://localhost:8080),
//     with DEV_ONLY_NO_AUTH=1 and at least one interface the dev user can access;
//   • `google-chrome` (or set CHROME=/path/to/chrome) is on PATH.
//
// Usage:
//   node server/scripts/regen-tutorial-screenshots.mjs
//   APP_URL=http://localhost:8080 CHROME=chromium node server/scripts/regen-tutorial-screenshots.mjs
//
// Writes app-{create,config}-{desktop,mobile}-{en,de}.png into public/tutorial-img/.
// The generic WireGuard-client screenshots (win-*, android-*) are NOT regenerated.

import { spawn } from 'node:child_process';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const APP = process.env.APP_URL || 'http://localhost:8080';
const CHROME = process.env.CHROME || 'google-chrome';
const OUT = join(dirname(fileURLToPath(import.meta.url)), '..', 'public', 'tutorial-img');
const PORT = 9456;
const sleep = ms => new Promise(r => setTimeout(r, ms));

// Per-language button/label text (the app UI is translated, so we can't match
// on English text once the language is German).
const LANGS = {
  en: { tag: 'en-US', generate: 'Generate', delete: 'Delete', panel: 'Scan the QR', download: 'Download' },
  de: { tag: 'de-DE', generate: 'Erstellen', delete: 'Löschen', panel: 'Scanne den QR', download: 'Herunterladen' },
};
const VIEWPORTS = {
  desktop: { width: 1280, height: 850, mobile: false },
  mobile: { width: 430, height: 900, mobile: true },
};

const profile = mkdtempSync(join(tmpdir(), 'wg-shots-'));
const chrome = spawn(CHROME, [
  '--headless=new', '--disable-gpu', '--hide-scrollbars', '--no-first-run',
  '--no-default-browser-check', `--remote-debugging-port=${PORT}`,
  `--user-data-dir=${profile}`, 'about:blank',
], { stdio: 'ignore' });

async function cdp() {
  for (let i = 0; i < 40; i++) {
    try { return await (await fetch(`http://localhost:${PORT}/json/version`)).json(); }
    catch { await sleep(250); }
  }
  throw new Error('chrome CDP did not come up');
}

const ver = await cdp();
const ws = new WebSocket(ver.webSocketDebuggerUrl);
await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });

let id = 0;
const pending = new Map();
ws.onmessage = m => {
  const x = JSON.parse(m.data);
  if (x.id && pending.has(x.id)) {
    const p = pending.get(x.id); pending.delete(x.id);
    x.error ? p.reject(new Error(JSON.stringify(x.error))) : p.resolve(x.result);
  }
};
const send = (method, params = {}, sessionId) =>
  new Promise((resolve, reject) => { const mid = ++id; pending.set(mid, { resolve, reject }); ws.send(JSON.stringify({ id: mid, method, params, sessionId })); });

const { targetId } = await send('Target.createTarget', { url: 'about:blank' });
const { sessionId } = await send('Target.attachToTarget', { targetId, flatten: true });
const S = (m, p) => send(m, p, sessionId);
await S('Page.enable'); await S('Runtime.enable');

async function evalJS(expression) {
  const r = await S('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true });
  if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description || 'eval error');
  return r.result.value;
}
async function waitFor(expr, timeout = 20000) {
  const t0 = Date.now();
  while (Date.now() - t0 < timeout) { if (await evalJS(expr)) return; await sleep(200); }
  throw new Error('timeout: ' + expr);
}
const clickByText = txt =>
  evalJS(`(()=>{const b=[...document.querySelectorAll('button')].find(b=>b.textContent.trim()===${JSON.stringify(txt)});if(b){b.click();return true}return false})()`);
async function shot(file) {
  const { data } = await S('Page.captureScreenshot', { format: 'png' });
  writeFileSync(join(OUT, file), Buffer.from(data, 'base64'));
  console.log('wrote', file);
}

// Numbered step markers (orange badges) overlaid on a target element before a
// shot. `elExpr` is a JS expression evaluating to the element (or null → skip).
async function mark(elExpr, label) {
  return evalJS(`(()=>{const el=(${elExpr}); if(!el) return false;
    const r=el.getBoundingClientRect();
    const b=document.createElement('div'); b.className='__wgmark'; b.textContent=${JSON.stringify(String(label))};
    Object.assign(b.style,{position:'fixed',left:(r.left-16)+'px',top:(r.top-16)+'px',width:'30px',height:'30px',
      borderRadius:'9999px',background:'#e8552b',color:'#fff',display:'flex',alignItems:'center',justifyContent:'center',
      font:'700 16px system-ui,sans-serif',boxShadow:'0 2px 8px rgba(0,0,0,.4)',zIndex:'99999',pointerEvents:'none',border:'2px solid #fff'});
    document.body.appendChild(b); return true;})()`);
}
const clearMarks = () => evalJS(`document.querySelectorAll('.__wgmark').forEach(e=>e.remove())`);
const byText = (tag, txt) => `[...document.querySelectorAll('${tag}')].find(e=>e.textContent.trim()===${JSON.stringify(txt)})`;

// Force the browser locale before loading so the app picks it up from
// navigator.language on first render — no reload, no localStorage. We override
// navigator.language via an init script (Emulation.setLocaleOverride doesn't
// reliably change navigator.language in headless, and the host locale would
// otherwise win).
let langScriptId = null;
async function useLang(L, V) {
  if (langScriptId) {
    await S('Page.removeScriptToEvaluateOnNewDocument', { identifier: langScriptId }).catch(() => {});
  }
  const src = `Object.defineProperty(navigator,'language',{get:()=>'${L.tag}',configurable:true});`
    + `Object.defineProperty(navigator,'languages',{get:()=>['${L.tag}'],configurable:true});`;
  langScriptId = (await S('Page.addScriptToEvaluateOnNewDocument', { source: src })).identifier;
  await S('Emulation.setDeviceMetricsOverride', { ...V, deviceScaleFactor: 2, screenWidth: V.width, screenHeight: V.height });
}

async function capture(langKey, vpKey) {
  const L = LANGS[langKey], V = VIEWPORTS[vpKey];
  await useLang(L, V);
  await S('Page.navigate', { url: APP });
  await waitFor(byText('button', L.generate));
  await sleep(400);

  // Reset to a clean state: delete every existing device.
  while (await clickByText(L.delete)) await sleep(400);
  await sleep(400);
  // A device name is required — focus the input and type with real (trusted)
  // input so Dioxus's oninput handler updates the signal.
  await evalJS(`document.querySelector('input').focus()`);
  await S('Input.insertText', { text: 'laptop' });
  await waitFor(`document.querySelector('input')?.value==='laptop'`);
  await sleep(200);
  await evalJS('window.scrollTo(0,0)');
  // Markers: ① name the device, ② generate.
  await clearMarks();
  await mark(`document.querySelector('input')`, '1');
  await mark(byText('button', L.generate), '2');
  await sleep(150);
  await shot(`app-create-${vpKey}-${langKey}.png`);
  await clearMarks();

  // Generate one device → the config panel appears.
  await clickByText(L.generate);
  await waitFor(`document.body.innerText.includes(${JSON.stringify(L.panel)})`);
  await sleep(600);
  await evalJS(`(document.querySelector('svg')?.closest('.border-brand-soft')||document.querySelector('pre'))?.scrollIntoView({block:'center'})`);
  await sleep(300);
  // Marker: ① download the config.
  await clearMarks();
  await mark(byText('button', L.download), '1');
  await sleep(150);
  await shot(`app-config-${vpKey}-${langKey}.png`);
  await clearMarks();
}

// The live WireGuard install page (desktop), per language, with a marker on the
// Windows download. Best-effort — needs internet; skipped on failure.
async function captureWgInstall(langKey) {
  const L = LANGS[langKey];
  await useLang(L, VIEWPORTS.desktop);
  try {
    await S('Page.navigate', { url: 'https://www.wireguard.com/install/' });
    await waitFor(`document.readyState==='complete' && document.querySelectorAll('a').length>3`, 15000);
    await sleep(800);
    await evalJS('window.scrollTo(0,0)');
    await clearMarks();
    // Prefer the "Download Windows Installer" button; fall back to any Windows link.
    await mark(`[...document.querySelectorAll('a')].find(a=>/download.*windows/i.test(a.textContent))||[...document.querySelectorAll('a')].find(a=>/windows/i.test(a.textContent))`, '1');
    await sleep(150);
    await shot(`wg-install-desktop-${langKey}.png`);
    await clearMarks();
    return true;
  } catch (e) {
    console.log(`  wireguard.com capture (${langKey}) failed — offline?`, e.message);
    return false;
  }
}

try {
  for (const lang of ['en', 'de'])
    for (const vp of ['desktop', 'mobile'])
      await capture(lang, vp);
  for (const lang of ['en', 'de'])
    await captureWgInstall(lang);
  console.log('done');
} finally {
  await send('Target.closeTarget', { targetId }).catch(() => {});
  ws.close();
  chrome.kill('SIGTERM');
  await sleep(500);
  try { rmSync(profile, { recursive: true, force: true }); } catch { /* chrome still releasing files */ }
}
