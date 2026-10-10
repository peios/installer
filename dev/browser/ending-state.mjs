// The real recovery controller with in-memory fetch, time and navigation.
// No browser, SDK, daemon, network request or actual navigation is used.
//   node --experimental-vm-modules --test dev/browser/ending-state.mjs
// ENDING_SOURCE may select another ending.js for a baseline regression run.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';

const sourcePath = process.env.ENDING_SOURCE || new URL('../../page/ending.js', import.meta.url);
const source = readFileSync(sourcePath, 'utf8');
const flush = async () => { for (let n = 0; n < 20; n++) await Promise.resolve(); };
const deferred = () => {
    let resolve, reject;
    const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
    return { promise, resolve, reject };
};
const unavailable = () => ({ status: 503, ok: false });
const opaque = () => ({ status: 0, ok: false, type: 'opaque' });
const answer = (body) => ({ status: 200, ok: true, json: async () => body });
const page = (address) => ({ phases: address ? [{ ref: 'phase.network', detail: { address } }] : [] });

async function fixture() {
    const calls = [], states = [], navigations = [], timers = new Map();
    let clock = 0, nextTimer = 0;
    const context = vm.createContext({
        Date: class extends Date { static now() { return clock; } },
        location: { protocol: 'https:', hostname: '192.0.2.10', port: '8443' },
        AbortSignal: { timeout: (ms) => ({ timeout: ms }) },
        fetch: (url, options) => {
            const reply = deferred();
            calls.push({ url, options, ...reply });
            return reply.promise;
        },
        setTimeout: (run, delay) => { const id = ++nextTimer; timers.set(id, { run, delay }); return id; },
        clearTimeout: (id) => timers.delete(id),
    });
    const module = new vm.SourceTextModule(source, { context, identifier: String(sourcePath) });
    await module.link(() => { throw new Error('Unexpected import'); });
    await module.evaluate();
    const ending = module.namespace.createEnding({ onChange: (state) => states.push({ ...state }), go: (url) => navigations.push(url) });
    return {
        ending, calls, states, navigations, timers,
        async tick() {
            assert.equal(timers.size, 1, 'exactly one scheduled recovery round');
            const [id, timer] = [...timers][0]; timers.delete(id);
            clock += timer.delay; timer.run(); await flush();
        },
        now(value) { clock = value; },
    };
}
function unchanged(f, snapshot) {
    assert.deepEqual(f.navigations, snapshot.navigations, 'stale work cannot navigate');
    assert.deepEqual(f.states, snapshot.states, 'stale work cannot publish state');
    assert.deepEqual([...f.timers], snapshot.timers, 'stale work cannot schedule or replace a timer');
    assert.equal(f.calls.length, snapshot.calls, 'stale work cannot initiate another probe');
}
const snapshot = (f) => ({ navigations: [...f.navigations], states: [...f.states], timers: [...f.timers], calls: f.calls.length });
async function pendingJson(f, address) {
    const offset = f.calls.length, json = deferred();
    let parsing = false;
    f.ending.begin(page(address));
    f.calls[offset].resolve({ status: 200, ok: true, json: () => { parsing = true; return json.promise; } });
    if (address) f.calls[offset + 1].resolve(opaque());
    await flush();
    assert.equal(parsing, true, 'the actual controller is awaiting JSON');
    return json;
}
async function finishCurrent(f, offset, address) {
    f.calls[offset].resolve(address ? unavailable() : answer({ setup: false }));
    if (address) f.calls[offset + 1].resolve(opaque());
    await flush();
    assert.deepEqual(f.navigations, [address ? `https://${address.replace(/\/\d+$/, '')}:8443/` : '/']);
    assert.equal(f.ending.state.stage, 'going');
    assert.equal(f.timers.size, 0);
}

for (const body of [{ setup: false }, { setup: true }]) {
    test(`stop suppresses pending JSON fulfillment ${JSON.stringify(body)}`, async () => {
        const f = await fixture(), json = await pendingJson(f);
        f.ending.stop(); const before = snapshot(f);
        json.resolve(body); await flush();
        unchanged(f, before); assert.equal(f.ending.active, false);
    });
}
test('stop suppresses pending JSON rejection', async () => {
    const f = await fixture(), json = await pendingJson(f);
    f.ending.stop(); const before = snapshot(f);
    json.reject(new Error('fixture JSON rejection')); await flush();
    unchanged(f, before); assert.equal(f.ending.active, false);
});
for (const settle of ['fulfill', 'reject']) {
    test(`stop suppresses pending fetch ${settle}`, async () => {
        const f = await fixture(); f.ending.begin(page()); f.ending.stop();
        const before = snapshot(f);
        if (settle === 'fulfill') f.calls[0].resolve(answer({ setup: false }));
        else f.calls[0].reject(new Error('fixture transport rejection'));
        await flush(); unchanged(f, before); assert.equal(f.ending.active, false);
    });
}
for (const settle of ['non-setup', 'setup', 'reject']) {
    test(`new cycle ignores old JSON ${settle}, then completes normally`, async () => {
        const f = await fixture(), json = await pendingJson(f, '192.0.2.20/24');
        f.ending.stop();
        const offset = f.calls.length; f.ending.begin(page('192.0.2.30/24'));
        const before = snapshot(f);
        if (settle === 'reject') json.reject(new Error('fixture JSON rejection'));
        else json.resolve({ setup: settle === 'setup' });
        await flush(); unchanged(f, before); assert.equal(f.ending.active, true);
        await finishCurrent(f, offset, '192.0.2.30/24');
    });
}
for (const settle of ['non-setup', 'moved', 'reject']) {
    test(`new cycle ignores old fetch ${settle}, then completes normally`, async () => {
        const f = await fixture(); f.ending.begin(page('192.0.2.20/24'));
        f.ending.stop(); const offset = f.calls.length; f.ending.begin(page('192.0.2.30/24'));
        const before = snapshot(f);
        if (settle === 'reject') {
            f.calls[0].reject(new Error('fixture transport rejection'));
            f.calls[1].reject(new Error('fixture transport rejection'));
        } else {
            f.calls[0].resolve(settle === 'non-setup' ? answer({ setup: false }) : unavailable());
            f.calls[1].resolve(opaque());
        }
        await flush(); unchanged(f, before); assert.equal(f.ending.active, true);
        await finishCurrent(f, offset, '192.0.2.30/24');
    });
}
test('old setup JSON cannot schedule another round into an active cycle', async () => {
    const f = await fixture(), json = await pendingJson(f);
    f.ending.stop(); const offset = f.calls.length; f.ending.begin(page());
    const before = snapshot(f); json.resolve({ setup: true }); await flush();
    unchanged(f, before); await finishCurrent(f, offset);
});
test('multiple recovery cycles ignore old JSON in reverse settlement order', async () => {
    const f = await fixture(), old = [];
    for (let n = 0; n < 3; n++) { old.push(await pendingJson(f)); f.ending.stop(); }
    const offset = f.calls.length; f.ending.begin(page());
    const before = snapshot(f);
    old[2].resolve({ setup: false }); old[1].reject(new Error('fixture JSON rejection')); old[0].resolve({ setup: true });
    await flush(); unchanged(f, before); await finishCurrent(f, offset);
});
test('late old JSON cannot navigate again after the current cycle succeeds', async () => {
    const f = await fixture(), json = await pendingJson(f);
    f.ending.stop(); const offset = f.calls.length; f.ending.begin(page());
    await finishCurrent(f, offset); const before = snapshot(f);
    json.resolve({ setup: false }); await flush(); unchanged(f, before);
});
test('stale timer callback cannot start a round or lose the new cycle timer', async () => {
    const f = await fixture(); f.ending.begin(page());
    f.calls[0].resolve(answer({ setup: true })); await flush();
    const stale = [...f.timers.values()][0].run;
    f.ending.stop(); f.ending.begin(page());
    f.calls[1].resolve(answer({ setup: true })); await flush();
    const before = snapshot(f); stale(); await flush(); unchanged(f, before);
    f.ending.stop(); assert.equal(f.timers.size, 0, 'stop still cancels the current cycle timer');
});
test('a new cycle timeout is not advanced by an old round', async () => {
    const f = await fixture(), json = await pendingJson(f);
    f.ending.stop(); f.now(200_000); const offset = f.calls.length; f.ending.begin(page());
    const before = snapshot(f); json.resolve({ setup: true }); await flush(); unchanged(f, before);
    f.calls[offset].resolve(unavailable()); await flush();
    assert.equal(f.ending.state.stage, 'waiting'); assert.equal(f.timers.size, 1);
    await f.tick(); await finishCurrent(f, offset + 1);
});

test('normal same-origin success still navigates exactly once', async () => {
    const f = await fixture(); f.ending.begin(page()); await finishCurrent(f, 0);
    assert.equal(f.states.filter(s => s.stage === 'going').length, 1);
});
test('current-cycle JSON rejection preserves the existing arrival fallback', async () => {
    const f = await fixture(), json = await pendingJson(f);
    json.reject(new Error('fixture JSON rejection')); await flush();
    assert.deepEqual(f.navigations, ['/']); assert.equal(f.timers.size, 0);
});
test('current same-origin success takes precedence over a moved response', async () => {
    const f = await fixture(); f.ending.begin(page('192.0.2.20/24'));
    f.calls[0].resolve(answer({ setup: false })); f.calls[1].resolve(opaque()); await flush();
    assert.deepEqual(f.navigations, ['/']);
});
test('current moved response retains protocol, port, no-cors and timeouts', async () => {
    const f = await fixture(); f.ending.begin(page('192.0.2.20/24'));
    assert.equal(f.calls[0].url, '/hello');
    assert.equal(f.calls[1].url, 'https://192.0.2.20:8443/hello');
    assert.equal(f.calls[1].options.mode, 'no-cors');
    for (const call of f.calls) { assert.equal(call.options.cache, 'no-store'); assert.equal(call.options.signal.timeout, 4000); }
    await finishCurrent(f, 0, '192.0.2.20/24');
});
test('current moved IPv6 response retains the bracketed destination', async () => {
    const f = await fixture(); f.ending.begin(page('2001:db8::20/64'));
    assert.equal(f.calls[1].url, 'https://[2001:db8::20]:8443/hello');
    f.calls[0].resolve(unavailable()); f.calls[1].resolve(opaque()); await flush();
    assert.deepEqual(f.navigations, ['https://[2001:db8::20]:8443/']);
});
test('setup answering again continues polling and then succeeds', async () => {
    const f = await fixture(); f.ending.begin(page()); f.calls[0].resolve(answer({ setup: true })); await flush();
    assert.deepEqual(f.navigations, []); assert.equal([...f.timers.values()][0].delay, 1500);
    await f.tick(); await finishCurrent(f, 1);
});
test('unavailable and rejected probes enter lost after patience and keep retrying', async () => {
    const f = await fixture(); f.ending.begin(page());
    f.now(120_001); f.calls[0].reject(new Error('fixture transport rejection')); await flush();
    assert.equal(f.ending.state.stage, 'lost'); assert.equal(f.ending.active, true); assert.deepEqual(f.navigations, []);
    await f.tick(); f.calls[1].resolve(unavailable()); await flush();
    assert.equal(f.states.filter(s => s.stage === 'lost').length, 1);
    await f.tick(); await finishCurrent(f, 2);
});
test('repeated begin while active does not start a second cycle', async () => {
    const f = await fixture(); f.ending.begin(page()); f.ending.begin(page('192.0.2.20/24'));
    assert.equal(f.calls.length, 1); assert.equal(f.states.length, 1); await finishCurrent(f, 0);
});
