import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { startServer, sourceRoot, policies } from './server.mjs';

test('loopback-only fixture serves exact assets, transformation, policies, and no command endpoints', async () => {
    const server = await startServer(0);
    try {
        assert.equal(server.address().address, '127.0.0.1');
        const origin = `http://127.0.0.1:${server.address().port}`;
        const source = readFileSync(join(sourceRoot, 'index.html'), 'utf8');
        const setup = await fetch(`${origin}/`);
        assert.equal(await setup.text(), source.replace('<html lang="en">', '<html lang="en" data-conversation="oobe">'));
        assert.equal(setup.headers.get('content-security-policy'), policies.oobe);
        assert.match(policies.oobe, /connect-src 'self' http:/);
        const install = await fetch(`${origin}/?conversation=install`);
        assert.equal(await install.text(), source);
        assert.equal(install.headers.get('content-security-policy'), policies.install);
        const files = [...readdirSync(sourceRoot).filter(name => /\.(js|css)$/.test(name)), ...readdirSync(join(sourceRoot, 'fonts')).filter(name => name.endsWith('.woff2')).map(name => `fonts/${name}`)];
        for (const name of files) {
            const response = await fetch(`${origin}/${name}`);
            assert.equal(response.status, 200, name);
            assert.deepEqual(Buffer.from(await response.arrayBuffer()), readFileSync(join(sourceRoot, name)), name);
            assert.equal(response.headers.get('content-security-policy'), policies.install);
        }
        for (const path of ['/live', '/hello', '/log.txt', '/src/page.rs', '/%2e%2e/src/page.rs', '/act.reboot']) assert.equal((await fetch(`${origin}${path}`)).status, 404);
        assert.equal((await fetch(`${origin}/`, { method: 'POST', body: 'act.reboot' })).status, 404);
    } finally { await new Promise(resolve => server.close(resolve)); }
});
