// Captures the settings pages of the real desktop app (dev mock, sample data).
import { open } from './cdp.mjs';
const OUT = process.argv[2];
const p = await open('http://localhost:1420/', { w: 1280, h: 800, dpr: 2 });
const full = (n) => p.shot(`${OUT}/${n}.png`);
const log = async (t) => console.log(t, '=>', (await p.texts()).split(' | ').filter((x) => !/Minimize|Maximize|Close|Theme/.test(x)).slice(4, 40).join(' | '));
console.log('modes tab', await p.click('Modes', { exact: true })); await p.sleep(500); await full('s-modes-display'); await log('modes');
console.log('touchpad', await p.click('Touchpad', { exact: true })); await p.sleep(600); await full('s-modes-touchpad'); await log('touchpad');
console.log('stylus', await p.click('Stylus (Graphics Tablet)')); await p.sleep(600); await full('s-modes-stylus'); await log('stylus');
console.log('android', await p.click('Android app', { exact: true })); await p.sleep(600); await full('s-android'); await log('android');
console.log('conn', await p.click('Connection', { exact: true })); await p.sleep(600); await full('s-connection'); await log('connection');
console.log('general', await p.click('General', { exact: true })); await p.sleep(600); await full('s-general');
console.log('advanced', await p.click('Advanced', { exact: true })); await p.sleep(600); await full('s-advanced'); await log('advanced');
console.log('health', await p.click('System health', { exact: true })); await p.sleep(600); await full('s-health');
console.log('device', await p.click('SM-X616B')); await p.sleep(800); await full('s-device'); await log('device');
await p.close();
