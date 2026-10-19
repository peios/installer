import assert from "node:assert/strict";
import test from "node:test";

import { connect, elsewhere } from "./cdp.mjs";

class Socket {
    readyState = 1;
    sent = [];
    closes = 0;
    send(text) {
        this.sent.push(JSON.parse(text));
    }
    close() {
        this.closes++;
        this.onclose?.();
    }
    reply(message) {
        this.onmessage({ data: JSON.stringify(message) });
    }
}

test("responses settle the matching request once and leave later requests independent", async () => {
    const socket = new Socket();
    const events = [];
    const client = await connect(socket, (event) => events.push(event));
    const first = client.send("first");
    const second = client.send("second");
    socket.reply({ id: 2, result: "two" });
    socket.reply({ id: 1, result: "one" });
    socket.reply({ id: 1, error: { message: "duplicate must be ignored" } });
    socket.reply({ method: "Page.ready" });
    assert.equal((await first).result, "one");
    assert.equal((await second).result, "two");
    assert.deepEqual(events, [{ method: "Page.ready" }]);
    client.close();
    assert.equal(socket.closes, 1);
});

test("disconnect rejects every pending request and future sends", async () => {
    const socket = new Socket();
    let cleanups = 0;
    const client = await connect(
        socket,
        () => {},
        1000,
        () => cleanups++,
    );
    const first = assert.rejects(client.send("first"), /closed/);
    const second = assert.rejects(client.send("second"), /closed/);
    socket.close();
    await Promise.all([first, second]);
    await assert.rejects(client.send("later"), /closed/);
    assert.equal(cleanups, 1);
});

test("a command timeout closes the session and rejects all waiting commands", async () => {
    const socket = new Socket();
    const client = await connect(socket, () => {}, 10);
    await Promise.all([
        assert.rejects(client.send("missing"), /timed out/),
        assert.rejects(client.send("also waiting"), /timed out/),
    ]);
    assert.equal(socket.closes, 1);
});

test("a socket that never opens has a bounded startup lifetime", async () => {
    const socket = new Socket();
    socket.readyState = 0;
    await assert.rejects(
        connect(socket, () => {}, 10),
        /connection timed out/,
    );
    assert.equal(socket.closes, 1);
});

test("protocol errors reject commands and malformed frames close the session", async () => {
    const socket = new Socket();
    const client = await connect(socket, () => {});
    const response = assert.rejects(client.send("invalid"), /unsupported/);
    socket.reply({ id: 1, error: { message: "unsupported" } });
    await response;
    const pending = assert.rejects(client.send("pending"), /invalid JSON/);
    socket.onmessage({ data: "{" });
    await pending;
});

test("synchronous send failure releases its pending request", async () => {
    const socket = new Socket();
    socket.send = () => {
        throw new Error("write failed");
    };
    const client = await connect(socket, () => {});
    await assert.rejects(client.send("first"), /write failed/);
    client.close();
});

test("request origin validation rejects prefixes, credentials tricks and malformed URLs", () => {
    const bad = [
        "http://localhost:77901/path",
        "http://localhost.evil:7790/path",
        "http://localhost:7790@evil/path",
        "not a URL",
    ];
    assert.deepEqual(
        elsewhere(
            [
                "http://localhost:7790/path",
                "ws://localhost:7790/live",
                "data:text/plain,hello",
                ...bad,
            ],
            "http://localhost:7790/",
        ),
        bad,
    );
});
