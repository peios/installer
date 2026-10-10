// Preloaded before app.js. No native WebSocket or fetch is ever called.
(() => {
    const sockets = [], requests = [], commands = [], unexpected = [], pending = [];
    let mode = 'failure';
    class FixtureSocket {
        static CONNECTING = 0; static OPEN = 1; static CLOSING = 2; static CLOSED = 3;
        constructor(url) {
            if (url !== `${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/live`) throw new Error(`Unexpected WebSocket ${url}`);
            this.url = url; this.readyState = FixtureSocket.CONNECTING; sockets.push(this);
        }
        send(data) { commands.push(JSON.parse(data)); }
        open() { this.readyState = FixtureSocket.OPEN; this.onopen?.(new Event('open')); }
        message(value) { this.onmessage?.(new MessageEvent('message', { data: JSON.stringify(value) })); }
        close() { this.readyState = FixtureSocket.CLOSED; this.onclose?.(new Event('close')); }
    }
    window.WebSocket = FixtureSocket;
    const response = (kind, there) => {
        if (kind === 'failure') throw new TypeError('Fixture transport failure; cause deliberately unspecified');
        if (kind === 'setup') return new Response(JSON.stringify({ setup: 'fixture', boot: 'fixture' }), { status: 200 });
        if (kind === 'opaque') return there ? { type: 'opaque', status: 0, ok: false } : new Response('', { status: 503 });
        throw new Error(`Unknown fixture response ${kind}`);
    };
    window.fetch = async (input, options = {}) => {
        const url = new URL(typeof input === 'string' ? input : input.url, location.href);
        const there = url.origin !== location.origin;
        const item = { url: url.href, mode: options.mode ?? null };
        requests.push(item);
        if (!there && url.pathname.startsWith('/.gxwi/')) return new Response('', { status: 200 });
        const fixtureHost = ['192.0.2.44', '[2001:db8::44]'].includes(url.hostname);
        if (url.pathname !== '/hello' || (there && !fixtureHost)) {
            unexpected.push(item); throw new Error(`Unmocked fetch blocked: ${url.href}`);
        }
        if (mode === 'pending') return new Promise((resolve, reject) => pending.push({ there, resolve, reject }));
        return response(mode, there);
    };
    window.__recovery = {
        open(index = sockets.length - 1) { sockets[index].open(); },
        state(value, index = sockets.length - 1) { sockets[index].message(value); },
        close(index = sockets.length - 1) { sockets[index].close(); },
        mode(value) { mode = value; },
        resolvePending(kind = 'opaque') {
            for (const item of pending.splice(0)) {
                try { item.resolve(response(kind, item.there)); } catch (error) { item.reject(error); }
            }
        },
        snapshot() { return { sockets: sockets.length, requests: [...requests], commands: [...commands], unexpected: [...unexpected], pending: pending.length }; },
    };
})();
