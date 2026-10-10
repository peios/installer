// Test-only static assets; never starts an installer or accepts commands.
import { createServer } from 'node:http';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { fileURLToPath } from 'node:url';

export const repository = resolve(fileURLToPath(new URL('../../../', import.meta.url)));
export const sourceRoot = resolve(process.env.SOURCE_ROOT || join(repository, 'page'));
const rust = readFileSync(join(sourceRoot, '../src/page.rs'), 'utf8');
function policy(name) {
    const match = rust.match(new RegExp(`const ${name}: &str = "([\\s\\S]*?)";`));
    if (!match) throw new Error(`Missing production ${name}`);
    return match[1].replace(/\\\r?\n\s*/g, '');
}
export const policies = { install: policy('POLICY'), oobe: policy('SETUP_POLICY') };
export function documentHTML(conversation) {
    const html = readFileSync(join(sourceRoot, 'index.html'), 'utf8');
    if (!html.includes('<html lang="en">')) throw new Error('Production HTML transformation needs review');
    // Exactly src/page.rs::index, with only its first matching occurrence changed.
    return conversation === 'oobe' ? html.replace('<html lang="en">', '<html lang="en" data-conversation="oobe">') : html;
}
const assets = new Set(readdirSync(sourceRoot).filter(name => /\.(js|css)$/.test(name)).map(name => `/${name}`));
for (const name of readdirSync(join(sourceRoot, 'fonts'))) if (name.endsWith('.woff2')) assets.add(`/fonts/${name}`);
export function startServer(port = Number(process.env.RECOVERY_PORT || 7797)) {
    const server = createServer((request, response) => {
        const url = new URL(request.url, `http://127.0.0.1:${port}`);
        if (request.method !== 'GET' || (url.pathname !== '/' && !assets.has(url.pathname) && url.pathname !== '/__health')) {
            response.writeHead(404); response.end('Fixture has no daemon or command endpoints.'); return;
        }
        if (url.pathname === '/__health') { response.writeHead(200); response.end('recovery fixture'); return; }
        const conversation = url.searchParams.get('conversation') === 'install' ? 'install' : 'oobe';
        const document = url.pathname === '/';
        const contentType = document ? 'text/html; charset=utf-8' : url.pathname.endsWith('.js') ? 'text/javascript; charset=utf-8' : url.pathname.endsWith('.css') ? 'text/css; charset=utf-8' : 'font/woff2';
        response.writeHead(200, {
            'content-type': contentType,
            'content-security-policy': document ? policies[conversation] : policies.install,
            'x-content-type-options': 'nosniff', 'referrer-policy': 'no-referrer', 'cache-control': 'no-cache',
        });
        response.end(document ? documentHTML(conversation) : readFileSync(join(sourceRoot, url.pathname)));
    });
    return new Promise((resolve, reject) => {
        server.once('error', reject);
        server.listen(port, '127.0.0.1', () => resolve(server));
    });
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
    const server = await startServer();
    console.log(`Recovery fixture: http://127.0.0.1:${server.address().port}, assets: ${sourceRoot}`);
    for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, () => server.close(() => process.exit(0)));
}
