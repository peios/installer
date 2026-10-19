// DevTools request lifetimes are bounded, including startup and disconnection.
export const REQUEST_TIMEOUT_MS = 10_000;

export async function connect(ws, onEvent, timeoutMs = REQUEST_TIMEOUT_MS, onClose = () => {}) {
    let next = 1;
    let closed = false;
    const waiting = new Map();
    let opened;
    const ready = new Promise((resolve, reject) => {
        opened = { resolve, reject };
    });
    const startupTimer = setTimeout(
        () => fail(new Error("DevTools connection timed out")),
        timeoutMs,
    );

    function fail(error) {
        if (closed) return;
        closed = true;
        clearTimeout(startupTimer);
        opened.reject(error);
        for (const request of waiting.values()) {
            clearTimeout(request.timer);
            request.reject(error);
        }
        waiting.clear();
        ws.close();
        onClose();
    }

    ws.onopen = () => {
        clearTimeout(startupTimer);
        opened.resolve();
    };
    ws.onerror = () => fail(new Error("DevTools connection failed"));
    ws.onclose = () => fail(new Error("DevTools connection closed"));
    ws.onmessage = ({ data }) => {
        let message;
        try {
            message = JSON.parse(data);
        } catch {
            fail(new Error("DevTools returned invalid JSON"));
            return;
        }
        if (!message.id) return onEvent(message);
        const request = waiting.get(message.id);
        if (!request) return;
        waiting.delete(message.id);
        clearTimeout(request.timer);
        if (message.error) {
            request.reject(new Error(message.error.message ?? "DevTools command failed"));
        } else {
            request.resolve(message);
        }
    };
    if (ws.readyState === 1) ws.onopen();
    await ready;

    function send(method, params = {}) {
        if (closed) return Promise.reject(new Error("DevTools connection is closed"));
        return new Promise((resolve, reject) => {
            const id = next++;
            const timer = setTimeout(() => {
                fail(new Error(`DevTools ${method} timed out`));
            }, timeoutMs);
            waiting.set(id, { resolve, reject, timer });
            try {
                ws.send(JSON.stringify({ id, method, params }));
            } catch (error) {
                waiting.delete(id);
                clearTimeout(timer);
                reject(error);
            }
        });
    }
    return { send, close: () => fail(new Error("DevTools client closed")) };
}

/** HTTP and WebSocket traffic may share the exact authority, never a prefix. */
export function elsewhere(requests, site) {
    const origin = new URL(site).origin;
    const websocketOrigin = origin.replace(/^http/, "ws");
    return requests.filter((text) => {
        try {
            const url = new URL(text);
            return (
                url.protocol !== "data:" && url.origin !== origin && url.origin !== websocketOrigin
            );
        } catch {
            return true;
        }
    });
}
