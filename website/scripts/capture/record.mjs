// Records real interactions with the desktop app (dev mock) as a screencast and writes ffconcat lists for ffmpeg.
import { open } from './cdp.mjs';
import { writeFileSync, mkdirSync } from 'node:fs';
const which = process.argv[2];
const OUT = process.argv[3];
mkdirSync(OUT, { recursive: true });
const p = await open('http://localhost:1420/', { w: Number(process.env.VW || 1280), h: Number(process.env.VH || 800), dpr: 1 });
const start = async () => { p.frames.length = 0; await p.send('Page.startScreencast', { format: 'jpeg', quality: 88, everyNthFrame: 1 }); };
const stop = async () => { await p.send('Page.stopScreencast'); };
const hold = (ms) => p.sleep(ms);

if (which === 'wizard') {
  await start(); await hold(900);
  await p.click('Add device'); await hold(1300);
  await p.click('Setup via USB'); await hold(2200);
  await p.click('Next'); await hold(1200);
  await p.click('Setup this device'); await hold(1600);
  await p.click('Next'); await hold(2800);
  await p.click('Next'); await hold(1300);
  await p.click('Configure Wireless over USB'); await hold(3600);
  await p.click('Next'); await hold(2500);
  await stop();
} else {
  await start(); await hold(1200);
  await p.click('Modes', { exact: true }); await hold(1500);
  await p.click('Touchpad', { exact: true }); await hold(2200);
  await p.click('Stylus (Graphics Tablet)'); await hold(2200);
  await p.click('Mouse', { exact: true }); await hold(1200);
  await p.click('Graphics tablet', { exact: true }); await hold(1200);
  await p.click('Dynamic (experimental)', { exact: true }); await hold(1500);
  await p.click('Android app', { exact: true }); await hold(3000);
  await stop();
}
const fr = p.frames;
const lines = ['ffconcat version 1.0'];
fr.forEach((f, i) => {
  const name = `${OUT}/f${String(i).padStart(4, '0')}.jpg`;
  writeFileSync(name, Buffer.from(f.data, 'base64'));
  const dt = i < fr.length - 1 ? fr[i + 1].metadata.timestamp - f.metadata.timestamp : 1.0;
  lines.push(`file '${name}'`, `duration ${Math.max(0.01, dt).toFixed(3)}`);
});
lines.push(`file '${OUT}/f${String(fr.length - 1).padStart(4, '0')}.jpg'`);
writeFileSync(`${OUT}/list.txt`, lines.join('\n') + '\n');
console.log(which, 'frames', fr.length);
await p.close();
