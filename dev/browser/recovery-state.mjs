// Run with node --experimental-vm-modules dev/browser/recovery-state.mjs
// SOURCE_ROOT may point to a baseline page/ directory for a red regression run.
// RECOVERY_RESULTS selects the JSON evidence path (default: out/recovery-state.json).
// Loads the actual app/progress/ending/bits ES modules. Only browser
// primitives, timers, network, and unrelated visual/page modules are mocked.
// Never creates a socket, sends a request, navigates, or runs a daemon.
import vm from 'node:vm';
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import assert from 'node:assert/strict';
const root = resolve(process.env.SOURCE_ROOT || fileURLToPath(new URL('../../page', import.meta.url)));
const resultPath = resolve(process.env.RECOVERY_RESULTS || 'out/recovery-state.json');
const sourceFiles = ['app.js', 'progress.js', 'ending.js', 'bits.js'];
const sources = Object.fromEntries(sourceFiles.map(f => [f, readFileSync(`${root}/${f}`, 'utf8')]));
const sha256 = Object.fromEntries(sourceFiles.map(f => [f, createHash('sha256').update(sources[f]).digest('hex')]));

class Element {
  constructor(tag, document) {
    this.tagName = tag.toUpperCase(); this.ownerDocument = document;
    this.children = []; this.parentNode = null; this.dataset = {};
    this.attributes = new Map(); this.className = ''; this.hidden = false;
    this.style = { setProperty() {} }; this.events = new Map();
    this.scrollTop = 0; this.scrollHeight = 0; this.clientHeight = 100;
    const tokens = () => this.className.split(/\s+/).filter(Boolean);
    this.classList = {
      contains: x => tokens().includes(x),
      add: (...xs) => { this.className = [...new Set([...tokens(), ...xs])].join(' '); },
      remove: (...xs) => { this.className = tokens().filter(x => !xs.includes(x)).join(' '); },
      toggle: (x, yes = !tokens().includes(x)) => { yes ? this.classList.add(x) : this.classList.remove(x); return yes; },
    };
    if (tag === 'template') this.content = { firstElementChild: new Element('svg', document) };
  }
  get isConnected() { return this === this.ownerDocument.body || !!this.parentNode?.isConnected; }
  get firstChild() { return this.children[0]; }
  set textContent(text) { this.replaceChildren(String(text)); }
  get textContent() { return this.children.map(c => typeof c === 'string' ? c : c.textContent).join(''); }
  append(...children) { for (const c of children) { if (c instanceof Element) { c.remove(); c.parentNode = this; } this.children.push(typeof c === 'object' ? c : String(c)); } }
  replaceChildren(...children) { for (const c of this.children) if (c instanceof Element) c.parentNode = null; this.children = []; this.append(...children); }
  remove() { if (this.parentNode) this.parentNode.children = this.parentNode.children.filter(c => c !== this); this.parentNode = null; }
  setAttribute(k, v) { v = String(v); this.attributes.set(k, v); if(k.startsWith('data-')) this.dataset[k.slice(5)] = v; if(k === 'class') this.className = v; if(k === 'href') this.href = v; }
  getAttribute(k) { return this.attributes.get(k) ?? null; }
  removeAttribute(k) { this.attributes.delete(k); }
  addEventListener(k, f) { this.events.set(k, [...(this.events.get(k) ?? []), f]); }
  focus() { this.ownerDocument.activeElement = this; }
  matches(selector) {
    if(selector.startsWith('.')) return this.classList.contains(selector.slice(1));
    const attr = selector.match(/^\[([^=]+)="([^"]+)"\]$/);
    if(attr) return this.getAttribute(attr[1]) === attr[2];
    return this.tagName.toLowerCase() === selector;
  }
  querySelectorAll(selector) { return this.children.flatMap(c => c instanceof Element ? [...(c.matches(selector) ? [c] : []), ...c.querySelectorAll(selector)] : []); }
  querySelector(selector) { return this.querySelectorAll(selector)[0] ?? null; }
}
function dom(firstBoot) {
  const doc = { createElement: tag => new Element(tag, doc), createElementNS: (_ns, tag) => new Element(tag, doc), title: '' };
  doc.body = new Element('body', doc); doc.activeElement = doc.body;
  doc.documentElement = { dataset: { conversation: firstBoot ? 'oobe' : 'installer' } };
  const ids = Object.fromEntries(['stage','intro-stack','intro-lockup','head-lockup','edition','release','page','turn','ticker','status','status-text','toast','say','flash','field','skip','replay'].map(id => [id, doc.createElement('div')]));
  for(const node of Object.values(ids)) doc.body.append(node);
  doc.getElementById = id => ids[id] ?? null;
  doc.querySelectorAll = selector => doc.body.querySelectorAll(selector);
  return { doc, ids };
}
async function fixture({firstBoot = true, protocol = 'https:', hostname = '192.0.2.10', port = '8443'} = {}) {
  const {doc, ids} = dom(firstBoot);
  let now = 0, nextTimer = 0;
  const timers = new Map(), sockets = [], requests = [], navigations = [], sends = [];
  let reply = async () => { throw new TypeError('fixture: transport failure; cause unspecified'); };
  const setTimer = (fn, ms = 0) => { const id = ++nextTimer; timers.set(id, {at: now + ms, fn}); return id; };
  const flush = async () => { for(let i = 0; i < 15; i++) await Promise.resolve(); };
  async function advance(ms) {
    const until = now + ms;
    while(true) {
      const entry = [...timers].filter(([, t]) => t.at <= until).sort((a,b) => a[1].at - b[1].at)[0];
      if(!entry) break;
      now = entry[1].at; timers.delete(entry[0]); entry[1].fn(); await flush();
    }
    now = until; await flush();
  }
  class MockSocket {
    static OPEN = 1;
    constructor(url) { this.url = url; this.readyState = 0; sockets.push(this); }
    open() { this.readyState = 1; this.onopen?.(); }
    message(view) { this.onmessage?.({data: JSON.stringify(view)}); }
    close() { this.readyState = 3; this.onclose?.(); }
    send(text) { sends.push(text); }
  }
  const ctx = vm.createContext({
    console, document: doc, location: { protocol, hostname, port, host: hostname + (port ? `:${port}` : ''), assign: url => navigations.push(url) },
    window: { addEventListener() {} }, matchMedia: () => ({matches: true}),
    WebSocket: MockSocket, sessionStorage: { getItem() { return null; }, removeItem() {} },
    setTimeout: setTimer, clearTimeout: id => timers.delete(id), setInterval: setTimer, clearInterval: id => timers.delete(id),
    requestAnimationFrame: fn => setTimer(fn, 16), cancelAnimationFrame: id => timers.delete(id),
    Date: class extends Date { static now() { return now; } },
    AbortSignal: { timeout: ms => ({fixtureTimeout: ms}) },
    fetch: async (url, options) => { requests.push({url, cache: options.cache, mode: options.mode ?? 'same-origin', timeout: options.signal.fixtureTimeout}); return reply(url, options); },
  });
  const noPage = () => ({ draw() {}, gone() {}, focus() {}, arrived() {} });
  const mocked = {
    'account.js': {createAccountPage: noPage}, 'confirm.js': {createConfirmPage: noPage}, 'disk.js': {createDiskPage: noPage},
    'manual.js': {createManualPage: noPage}, 'naming.js': {createNamingPage: noPage}, 'network.js': {createNetworkPage: noPage}, 'welcome.js': {createWelcomePage: noPage},
    'field.js': {createField: () => ({even() {}, finale() { return true; }, stall() {}, stream() {}, hold() {}, wake() {}})},
    'intro.js': {createIntro: () => ({settled: true, start() {}, wake() {}, skip() {}})},
    'restart.js': {createRestart: () => ({active: false, begin() { throw Error('unexpected restart'); }})},
    'outro.js': {createOutro: () => ({complete() {}, leave: url => navigations.push(url)})},
  };
  const modules = new Map();
  async function load(name) {
    if(modules.has(name)) return modules.get(name);
    let mod;
    if(name in sources) mod = new vm.SourceTextModule(sources[name], {context: ctx, identifier: `${root}/${name}`});
    else { const exports = mocked[name]; assert.ok(exports, `Unexpected module ${name}`); mod = new vm.SyntheticModule(Object.keys(exports), function() { for(const [k,v] of Object.entries(exports)) this.setExport(k,v); }, {context:ctx}); }
    modules.set(name, mod);
    if(name in sources) await mod.link(specifier => load(specifier.replace('./','')));
    return mod;
  }
  const app = await load('app.js'); await app.evaluate(); await flush();
  function snapshot() {
    const turn = ids.turn;
    const visible = node => !!node?.isConnected && !node.hidden && (!node.parentNode || visible(node.parentNode));
    const control = way => { const node = turn.querySelector(`[data-way="${way}"]`); return {present: !!node, hidden: node?.hidden, visible: visible(node), text: node?.textContent ?? null, href: node?.href ?? null}; };
    const going = turn.querySelector('.going'), nav = turn.querySelector('.nav');
    return {kind: turn.dataset.kind, title: turn.querySelector('h1')?.textContent ?? null, finished: turn.classList.contains('finished'), stopped: turn.classList.contains('stopped'), percent: turn.querySelector('.pct')?.getAttribute('aria-valuenow') ?? null, navHidden: nav?.hidden ?? null, signIn: control('sign-in'), again: control('again'), reboot: control('reboot'), going: {text: going?.textContent ?? null, hidden: going?.hidden ?? null, visible: visible(going)}, status: ids['status-text'].textContent, announcement: ids.say.textContent, navigations: [...navigations], sends: [...sends]};
  }
  return {ids, sockets, requests, navigations, sends, advance, flush, snapshot, reply: fn => {reply = fn;}};
}
const page = ({address = '192.0.2.44/24', ended = null, job = 'setup'} = {}) => ({kind:'progress', job, title: job === 'setup' ? 'Finishing setup' : 'Installing Peios', summary: 'Applying the settings.', disk:null, phases:[{ref:'phase.account',name:'Creating your account',value:100,max:100},{ref:'phase.hostname',name:'Naming the machine',value:100,max:100}, ...(address ? [{ref:'phase.network',name:'Addressing eth0',value:0,max:100,detail:{address}}] : [])],lines:['Fixture only; no installer or network action.'],said:1,ended});
const view = page => ({page, seq:6, link:{state:'connected',daemon:page.job === 'setup' ? 'oobed' : 'installerd'},trail:[],waiting:null});
const done = {outcome:'complete',message:'Setup is complete. You can log in now.'};
// Commit identifies the checkout, hashes identify the exact tested scripts. A
// dirty candidate is never represented as unchanged baseline source.
const git = (...args) => { try { return execFileSync('git', ['-C', root, ...args], {encoding:'utf8', stdio:['ignore','pipe','ignore']}).trim(); } catch { return null; } };
const out = {sourceRoot:root, sourceCommit:git('rev-parse', 'HEAD'), sourceStatus:git('status', '--porcelain', '--', '.'), sha256, scenarios:{}, assertions:[], controlFailures:[]};
// Keep partial snapshots if a control assertion aborts before the final report.
process.on('exit', () => { mkdirSync(dirname(resultPath), {recursive:true}); writeFileSync(resultPath, JSON.stringify(out,null,2)+'\n'); });
function check(name, holds) { if (!holds) out.controlFailures.push(name); assert.ok(holds, name); out.assertions.push(name); }

const moved = await fixture();
moved.sockets[0].open(); moved.sockets[0].message(view(page())); await moved.advance(16);
out.scenarios.running = moved.snapshot();
check('ordinary running setup hides all end controls', !out.scenarios.running.signIn.visible && !out.scenarios.running.again.visible && !out.scenarios.running.reboot.visible);
moved.sockets[0].close(); await moved.flush(); await moved.advance(60);
out.scenarios.partialWaiting = moved.snapshot();
check('actual websocket-close handler starts cross-origin following', moved.requests.some(r => r.url === 'https://192.0.2.44:8443/hello' && r.mode === 'no-cors'));
check('no synthetic completion is introduced', !out.scenarios.partialWaiting.finished && out.scenarios.partialWaiting.title === 'Finishing setup' && out.scenarios.partialWaiting.percent !== '100');
check('unfinished setup cannot expose restart or start-again controls', !out.scenarios.partialWaiting.again.visible && !out.scenarios.partialWaiting.reboot.visible);
check('network failure alone does not navigate', moved.navigations.length === 0 && moved.sends.length === 0);
await moved.advance(121500);
out.scenarios.partialLost = moved.snapshot();
check('lost state is actually reached and announced', out.scenarios.partialLost.announcement.includes('Nothing has answered at 192.0.2.44:8443'));
moved.sockets.at(-1).open(); await moved.advance(60);
out.scenarios.reconnected = moved.snapshot();
check('reconnect clears recovery status without completing the job', out.scenarios.reconnected.going.hidden && !out.scenarios.reconnected.signIn.visible && out.scenarios.reconnected.status === 'Connected to oobed' && !out.scenarios.reconnected.finished);
moved.sockets.at(-1).message(view(page({ended:done}))); await moved.advance(16);
out.scenarios.complete = moved.snapshot();
check('server-confirmed completion keeps the existing sign-in route and hides Start again', out.scenarios.complete.signIn.visible && out.scenarios.complete.signIn.text === 'Sign in' && !out.scenarios.complete.again.visible && !out.scenarios.complete.reboot.visible && out.scenarios.complete.finished && out.scenarios.complete.percent === '100');

const same = await fixture(); same.sockets[0].open(); same.sockets[0].message(view(page({address:null}))); same.sockets[0].close(); await same.advance(121501);
out.scenarios.sameOriginPartialLost = same.snapshot();
check('same-origin failure does not navigate or send', same.navigations.length === 0 && same.sends.length === 0);

const completed = await fixture(); completed.sockets[0].open(); completed.sockets[0].message(view(page({ended:done}))); completed.sockets[0].close(); await completed.advance(60);
out.scenarios.completeWaiting = completed.snapshot();
check('confirmed setup completion already shows the following link and text', out.scenarios.completeWaiting.signIn.visible && out.scenarios.completeWaiting.signIn.text === 'Sign in' && out.scenarios.completeWaiting.going.visible && !out.scenarios.completeWaiting.again.visible && out.scenarios.completeWaiting.status === 'Setup has finished');

const failed = await fixture(); failed.sockets[0].open(); failed.sockets[0].message(view(page({ended:{outcome:'failed',message:'Fixture failure'}}))); await failed.advance(16);
out.scenarios.failedBeforeClose = failed.snapshot();
check('failed setup keeps stopped styling and Start again without sign-in', out.scenarios.failedBeforeClose.stopped && out.scenarios.failedBeforeClose.again.visible && !out.scenarios.failedBeforeClose.signIn.visible && !out.scenarios.failedBeforeClose.going.visible);
failed.sockets[0].close(); await failed.advance(60);
out.scenarios.failedAfterClose = failed.snapshot();
check('failed setup does not enter automatic recovery', failed.requests.length === 0 && out.scenarios.failedAfterClose.kind === 'away');

const install = await fixture({firstBoot:false}); install.sockets[0].open(); install.sockets[0].message(view(page({job:'install'}))); install.sockets[0].close(); await install.advance(60);
out.scenarios.installAfterClose = install.snapshot();
check('ordinary installer disconnect does not enter setup recovery', install.requests.length === 0 && out.scenarios.installAfterClose.kind === 'away');

const ipv6 = await fixture(); ipv6.sockets[0].open(); ipv6.sockets[0].message(view(page({address:'2001:db8::44/64'}))); ipv6.sockets[0].close(); await ipv6.advance(60);
out.scenarios.ipv6Waiting = ipv6.snapshot();
check('existing URL formation preserves HTTPS, IPv6 brackets and explicit port', out.scenarios.ipv6Waiting.signIn.href === 'https://[2001:db8::44]:8443/');

const resume = await fixture(); resume.reply(async url => ({status:200,ok:true,json:async()=>({setup:true})})); resume.sockets[0].open(); resume.sockets[0].message(view(page({address:null}))); resume.sockets[0].close(); await resume.advance(60);
out.scenarios.setupAnswersAgain = resume.snapshot();
check('setup answering again does not auto-navigate', resume.navigations.length === 0);

const arrived = await fixture(); arrived.reply(async url => url === '/hello' ? {status:503,ok:false} : {type:'opaque',status:0,ok:false}); arrived.sockets[0].open(); arrived.sockets[0].message(view(page())); arrived.sockets[0].close(); await arrived.advance(60);
out.scenarios.opaqueResponse = arrived.snapshot();
check('unchanged existing opaque response logic requests the existing destination', arrived.navigations[0] === 'https://192.0.2.44:8443/');

const cancelled = await fixture(); cancelled.sockets[0].open(); cancelled.sockets[0].message(view(page({ended:{outcome:'cancelled',message:'Fixture cancellation'}}))); cancelled.sockets[0].close(); await cancelled.advance(60);
out.scenarios.cancelledAfterClose = cancelled.snapshot();
check('cancelled outcome remains stopped and does not expose a sign-in action', out.scenarios.cancelledAfterClose.stopped && !out.scenarios.cancelledAfterClose.signIn.visible && out.scenarios.cancelledAfterClose.again.visible);

// Repeated close notifications are delivered to the actual handler. begin()
// must not duplicate its active polling round or introduce an action.
const repeated = await fixture(); repeated.sockets[0].open(); repeated.sockets[0].message(view(page())); repeated.sockets[0].close(); await repeated.flush();
const firstRequests = repeated.requests.length; repeated.sockets[0].close(); await repeated.flush();
check('duplicate close does not begin a second recovery polling round', repeated.requests.length === firstRequests && repeated.navigations.length === 0 && repeated.sends.length === 0);

// A reconnect while probe responses are pending must cancel their effects.
const pending = await fixture(); let releaseHere, releaseThere;
pending.reply(url => new Promise(resolve => { if(url === '/hello') releaseHere = resolve; else releaseThere = resolve; }));
pending.sockets[0].open(); pending.sockets[0].message(view(page())); pending.sockets[0].close(); await pending.advance(1500); pending.sockets.at(-1).open();
releaseHere({status:503,ok:false}); releaseThere({status:0,ok:false,type:'opaque'}); await pending.flush();
out.scenarios.reconnectedBeforeProbeSettled = pending.snapshot();
check('reconnect before fetch settlement suppresses pending probe navigation', pending.navigations.length === 0 && !out.scenarios.reconnectedBeforeProbeSettled.signIn.visible);

// First-boot setup and a normal installation share the progress renderer;
// preserve install end actions and prove recovery words do not leak into it.
const installDone = await fixture({firstBoot:false});
installDone.sockets[0].open();
installDone.sockets[0].message(view(page({job:'install', ended:{outcome:'complete',message:'Peios is installed. Restart to use it.',reboot:{ref:'reboot',name:'Reboot now'},start:{ref:'start',name:'Back to the start'}}})));
await installDone.advance(16);
out.scenarios.installComplete = installDone.snapshot();
check('completed install preserves backend-offered reboot and start controls', out.scenarios.installComplete.reboot.visible && out.scenarios.installComplete.again.visible && !out.scenarios.installComplete.signIn.visible && !out.scenarios.installComplete.going.visible && out.scenarios.installComplete.finished);

// Re-enter recovery after reconnect to catch stale neutral/sign-in labels.
const cycle = await fixture(); cycle.sockets[0].open(); cycle.sockets[0].message(view(page())); cycle.sockets[0].close(); await cycle.advance(1500);
cycle.sockets.at(-1).open(); await cycle.advance(60);
cycle.sockets.at(-1).close(); await cycle.advance(60);
out.scenarios.repeatedRecovery = cycle.snapshot();
check('a new recovery cycle leaves original unfinished heading/progress intact', out.scenarios.repeatedRecovery.title === 'Finishing setup' && !out.scenarios.repeatedRecovery.finished && out.scenarios.repeatedRecovery.percent === '66');
check('all recovery cycles issue zero job commands', moved.sends.length === 0 && cycle.sends.length === 0);

// URL semantics belong to ending.js and remain unchanged by this UI patch.
for (const [name, options, address, expected] of [
  ['sameHost', {}, '192.0.2.10/24', '/'],
  ['httpDefaultPort', {protocol:'http:',port:''}, '192.0.2.44/24', 'http://192.0.2.44/'],
  ['sameIpv6Host', {hostname:'[2001:db8::10]'}, '2001:db8::10/64', '/'],
]) {
  const f = await fixture(options); f.sockets[0].open(); f.sockets[0].message(view(page({address}))); f.sockets[0].close(); await f.advance(60);
  out.scenarios[name] = f.snapshot();
  check(`${name} preserves the existing onward URL`, out.scenarios[name].signIn.href === expected);
}
check('probe cache, mode and timeout remain unchanged', moved.requests.every(r => r.cache === 'no-store' && r.timeout === 4000 && (r.url === '/hello' ? r.mode === 'same-origin' : r.mode === 'no-cors')));

const requirements = [
 ['partial wait exposes recovery text', out.scenarios.partialWaiting.going.visible],
 ['partial wait exposes manual destination', out.scenarios.partialWaiting.signIn.visible],
 ['unknown outcome labels navigation neutrally', out.scenarios.partialWaiting.signIn.text === 'Open this machine'],
 ['partial lost exposes recovery text', out.scenarios.partialLost.going.visible],
 ['partial lost exposes manual destination', out.scenarios.partialLost.signIn.visible],
 ['unconfirmed status does not claim completion', out.scenarios.partialWaiting.status === 'Waiting for the machine'],
 ['same-origin unknown outcome wording does not claim setup finished', out.scenarios.sameOriginPartialLost.announcement === 'The machine has not answered. Its own screen says where it is.'],
 ['repeated recovery restores the neutral manual link', out.scenarios.repeatedRecovery.signIn.visible && out.scenarios.repeatedRecovery.signIn.text === 'Open this machine'],
 ['recovery controls are individually gated', out.scenarios.partialWaiting.again.hidden && out.scenarios.partialWaiting.reboot.hidden],
 ['same-origin recovery exposes its existing local manual destination', out.scenarios.sameOriginPartialLost.signIn.visible && out.scenarios.sameOriginPartialLost.signIn.href === '/' && out.scenarios.sameOriginPartialLost.going.visible],
 ['unknown recovery never claims successful completion', !out.scenarios.partialWaiting.finished && !out.scenarios.partialLost.finished && !out.scenarios.partialLost.announcement.includes('Setup is complete')],
];
out.recoveryRequirements = requirements.map(([name, passed])=>({name,passed}));
console.log(JSON.stringify({sourceCommit:out.sourceCommit, sourceStatus:out.sourceStatus, assertionCount:out.assertions.length, requirements:out.recoveryRequirements, resultPath},null,2));
process.exitCode = requirements.every(([,passed])=>passed) ? 0 : 1;
