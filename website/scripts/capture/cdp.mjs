// Tiny CDP helper for the private headless Chrome on :9333. Opens a tab, offers ev/click/shot/texts.
import { writeFileSync } from 'node:fs';
export async function open(url, { w = 1280, h = 800, dpr = 2, scheme = 'dark' } = {}) {
  const list = await (await fetch('http://127.0.0.1:9333/json/new?about:blank', { method: 'PUT' })).json();
  const ws = new WebSocket(list.webSocketDebuggerUrl);
  await new Promise((r) => (ws.onopen = r));
  let id = 0; const pending = new Map(); const frames = [];
  ws.onmessage = (e) => {
    const m = JSON.parse(e.data);
    if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); }
    if (m.method === 'Page.screencastFrame') { frames.push(m.params); ws.send(JSON.stringify({ id: ++id, method: 'Page.screencastFrameAck', params: { sessionId: m.params.sessionId } })); }
  };
  const send = (method, params = {}) => new Promise((res) => { const i = ++id; pending.set(i, res); ws.send(JSON.stringify({ id: i, method, params })); });
  const ev = async (expr) => (await send('Runtime.evaluate', { expression: expr, awaitPromise: true, returnByValue: true })).result?.result?.value;
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  await send('Page.enable');
  await send('Emulation.setDeviceMetricsOverride', { width: w, height: h, deviceScaleFactor: dpr, mobile: false });
  await send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: scheme }] });
  await send('Page.navigate', { url });
  await sleep(2500);
  // Clicks (with real mouse events inside the headless page) the smallest visible element whose text starts with [text];
  // inside an open dialog only the dialog is searched.
  const click = async (text, { exact = false } = {}) => {
    const pt = await ev(`(() => { const t=${JSON.stringify(text)}; const ex=${exact};
      const root=document.querySelector('[role=dialog]')||document;
      const els=[...root.querySelectorAll('button,a,[role=button],[role=tab],[role=combobox],[role=option],li,label,div,span')].filter(e=>e.getClientRects().length&&(ex? e.textContent.trim()===t : e.textContent.trim().startsWith(t)&&e.textContent.trim().length<t.length+60));
      els.sort((a,b)=>a.textContent.length-b.textContent.length); if(!els.length) return null;
      els[0].scrollIntoView({block:'center'}); const r=els[0].getBoundingClientRect(); return {x:r.x+r.width/2,y:r.y+r.height/2,dis:!!els[0].closest('button')?.disabled}; })()`);
    if (!pt) return false;
    for (const type of ['mouseMoved', 'mousePressed', 'mouseReleased']) await send('Input.dispatchMouseEvent', { type, x: pt.x, y: pt.y, button: 'left', clickCount: 1 });
    return pt.dis ? 'disabled' : true;
  };
  const texts = () => ev(`[...new Set([...document.querySelectorAll('button,[role=tab],[role=combobox],[role=switch],input')].filter(e=>e.offsetParent).map(e=>(e.getAttribute('aria-label')||e.textContent||e.value||'').trim().slice(0,50)).filter(Boolean))].join(' | ')`);
  const pageText = () => ev(`document.body.innerText.slice(0,900)`);
  const shot = async (path, clip) => {
    await sleep(700);
    const p = { format: 'png' }; if (clip) p.clip = { ...clip, scale: 1 };
    const r = await send('Page.captureScreenshot', p);
    writeFileSync(path, Buffer.from(r.result.data, 'base64')); console.log('saved', path);
  };
  const rect = (sel) => ev(`(()=>{const e=document.querySelector(${JSON.stringify(sel)}); if(!e) return null; const r=e.getBoundingClientRect(); return {x:r.x,y:r.y,width:r.width,height:r.height}})()`);
  const close = async () => { try { await fetch(`http://127.0.0.1:9333/json/close/${list.id}`); } catch {} ws.close(); };
  return { ev, click, texts, pageText, shot, rect, sleep, send, frames, close };
}
