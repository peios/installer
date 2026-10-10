import { test as base, expect } from '@playwright/test';
import { readFileSync, readdirSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { join } from 'node:path';
import { repository, sourceRoot, policies } from './server.mjs';

const pageState = ({ address = '192.0.2.44/24', ended = null, job = 'setup' } = {}) => ({
    kind: 'progress', job, title: job === 'setup' ? 'Finishing setup' : 'Installing Peios',
    summary: 'Applying the settings.', disk: null,
    phases: [
        { ref: 'phase.account', name: 'Creating your account', value: 100, max: 100 },
        { ref: 'phase.hostname', name: 'Naming the machine', value: 100, max: 100 },
        { ref: 'phase.network', name: 'Addressing eth0', value: 0, max: 100, detail: address ? { address } : {} },
    ], lines: ['Fixture only; no installer or network action.'], said: 1, ended,
});
const view = page => ({ page, seq: 6, boot: 'fixture', link: { state: 'connected', daemon: page.job === 'setup' ? 'oobed' : 'installerd' }, trail: [], waiting: null });
const completed = { outcome: 'complete', message: 'Setup is complete. You can sign in now.' };
const linkOf = page => page.locator('[data-way="sign-in"]');
const goingOf = page => page.locator('.going');
const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
const sourceFiles = ['index.html', 'style.css', ...readdirSync(sourceRoot).filter(name => name.endsWith('.js'))];
const evidence = {
    sourceCommit: process.env.SOURCE_COMMIT || (process.env.SOURCE_ROOT ? null : execFileSync('git', ['rev-parse', 'HEAD'], { cwd: repository, encoding: 'utf8' }).trim()),
    sourceKind: process.env.SOURCE_COMMIT ? 'caller-supplied commit; exact tested bytes identified by hashes' : process.env.SOURCE_ROOT ? 'external source directory; commit unspecified' : 'working tree at source HEAD; hashes identify exact tested bytes',
    workingTreeStatus: execFileSync('git', ['status', '--short'], { cwd: repository, encoding: 'utf8' }),
    sha256: Object.fromEntries(sourceFiles.map(name => [name, sha256(readFileSync(join(sourceRoot, name)))])),
    policies,
};
const test = base.extend({
    fixture: async ({ page, context, baseURL }, use, testInfo) => {
        const errors = [], denied = [], navigations = [], snapshots = [];
        const allowedAssets = new Set(sourceFiles.map(name => `/${name}`));
        for (const name of readdirSync(join(sourceRoot, 'fonts'))) allowedAssets.add(`/fonts/${name}`);
        let started = false, marker = null, expectedCommands = [];
        page.on('pageerror', error => errors.push(error.message));
        page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
        await context.route('**/*', async route => {
            const request = route.request(), url = new URL(request.url());
            if (request.isNavigationRequest()) {
                if (!started && url.origin === baseURL && url.pathname === '/') { started = true; return route.continue(); }
                navigations.push(url.href);
                if (marker === url.href) return route.fulfill({ status: 200, contentType: 'text/html', body: '<!doctype html><title>Fixture destination</title><h1>Mock destination reached</h1>' });
                denied.push(url.href); return route.abort('blockedbyclient');
            }
            if (url.origin === baseURL && url.pathname === '/favicon.ico') return route.fulfill({ status: 204, body: '' });
            if (url.origin === baseURL && allowedAssets.has(url.pathname)) return route.continue();
            denied.push(url.href); return route.abort('blockedbyclient');
        });
        await page.clock.install({ time: new Date('2026-10-10T12:00:00Z') });
        await page.clock.pauseAt(new Date('2026-10-10T12:00:01Z'));
        await page.addInitScript({ path: new URL('./mock-transport.js', import.meta.url).pathname });
        const snapshot = async name => {
            const data = await page.evaluate(() => ({
                url: location.href, kind: document.querySelector('#turn')?.dataset.kind,
                heading: document.querySelector('#turn h1')?.textContent,
                status: document.querySelector('#status-text')?.textContent,
                progress: document.querySelector('.pct')?.getAttribute('aria-valuenow'),
                turnClass: document.querySelector('#turn')?.className,
                transport: window.__recovery?.snapshot(),
            }));
            snapshots.push({ name, ...data }); return data;
        };
        const fixture = {
            snapshots, navigations,
            async boot(options = {}) {
                const response = await page.goto(options.job === 'install' ? '/?conversation=install' : '/');
                expect(response.headers()['content-security-policy']).toBe(options.job === 'install' ? policies.install : policies.oobe);
                expect(await page.locator('html').evaluate(html => getComputedStyle(html).colorScheme)).toBe('dark');
                expect(await page.locator('html').getAttribute('data-conversation')).toBe(options.job === 'install' ? null : 'oobe');
                await page.evaluate(state => { window.__recovery.open(); window.__recovery.state(state); }, view(pageState(options)));
                // Reduced motion uses the production intro's own skip path. Ordinary motion explicitly skips it.
                if (await page.locator('#skip').isVisible()) await page.locator('#skip').click();
                await page.clock.runFor(1200);
                await expect(page.locator('#page')).toHaveClass(/live/);
                await expect(page.locator('#turn')).toHaveAttribute('data-kind', 'progress');
                await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
            },
            async close() { await page.evaluate(() => window.__recovery.close(0)); await page.clock.runFor(80); },
            async reconnect() { await page.clock.runFor(1600); await page.evaluate(() => window.__recovery.open()); await page.clock.runFor(80); },
            async state(options) { await page.evaluate(state => window.__recovery.state(state), view(pageState(options))); await page.clock.runFor(80); },
            async mode(mode) { await page.evaluate(value => window.__recovery.mode(value), mode); },
            async transport() { return page.evaluate(() => window.__recovery.snapshot()); },
            destination(url) { marker = new URL(url, baseURL).href; return marker; },
            commands(value) { expectedCommands = value; },
            snapshot,
            async pictures(name) {
                await snapshot(name);
                await testInfo.attach(`${name}-full`, { body: await page.screenshot({ fullPage: true, animations: 'disabled' }), contentType: 'image/png' });
                if (await page.locator('.nav.ends').isVisible()) await testInfo.attach(`${name}-recovery`, { body: await page.locator('.nav.ends').screenshot({ animations: 'disabled' }), contentType: 'image/png' });
            },
        };
        try { await use(fixture); }
        finally {
            const transport = await page.evaluate(() => window.__recovery?.snapshot()).catch(() => null);
            await testInfo.attach('source-and-state.json', { body: JSON.stringify({ ...evidence, browser: page.context().browser().version(), project: testInfo.project.name, snapshots, transport, errors, denied, navigations }, null, 2), contentType: 'application/json' });
            expect(errors, 'No production browser exceptions or console errors').toEqual([]);
            expect(denied, 'No unmocked requests or unsolicited navigation').toEqual([]);
            if (transport) {
                expect(transport.unexpected).toEqual([]);
                expect(transport.commands).toEqual(expectedCommands);
            }
        }
    },
});

async function recoveryVisible(page) {
    await expect(linkOf(page), 'PEI1411 recovery link is effectively hidden').toBeVisible();
    await expect(linkOf(page)).toHaveText('Open this machine');
    await expect(goingOf(page)).toBeVisible();
    // Playwright visibility includes hidden ancestors, but not opacity. Check opacity and clipping separately.
    await expect.poll(() => linkOf(page).evaluate(link => {
        for (let node = link; node; node = node.parentElement) {
            const style = getComputedStyle(node);
            if (node.hidden || style.display === 'none' || style.visibility !== 'visible' || Number(style.opacity) < 0.99) return false;
        }
        return link.getClientRects().length > 0;
    })).toBe(true);
    await linkOf(page).scrollIntoViewIfNeeded();
    expect(await linkOf(page).evaluate(link => {
        const rect = link.getBoundingClientRect();
        const hit = document.elementFromPoint(rect.x + rect.width / 2, rect.y + rect.height / 2);
        return rect.width > 0 && rect.left >= 0 && rect.right <= innerWidth && (hit === link || link.contains(hit));
    }), 'Recovery link is inside the viewport and pointer-hit-testable').toBe(true);
    await linkOf(page).click({ trial: true });
    // Reach it by normal tab order, without programmatic link focus or click.
    await page.locator('#turn h1').focus();
    for (let i = 0; i < 12 && !(await linkOf(page).evaluate(link => link === document.activeElement)); i++) await page.keyboard.press('Tab');
    await expect(linkOf(page)).toBeFocused();
}
async function unfinished(page) {
    await expect(page.locator('#turn h1')).toHaveText('Finishing setup');
    await expect(page.locator('#turn')).not.toHaveClass(/finished|stopped/);
    await expect(page.getByRole('progressbar', { name: 'Overall progress', exact: true })).toHaveAttribute('aria-valuenow', '66');
    await expect(page.locator('.phase').nth(2)).toHaveClass(/active/);
    await expect(page.locator('.phase').nth(2).getByRole('progressbar')).toHaveAttribute('aria-valuenow', '0');
    await expect(page.locator('[data-way="again"]')).toBeHidden();
    await expect(page.locator('[data-way="reboot"]')).toBeHidden();
    await expect(page.locator('#status-text')).toHaveText('Waiting for the machine');
    await expect(page.locator('#say')).not.toContainText('Setup is complete');
}

test('unfinished disconnect reveals recovery without completion @recovery-regression', async ({ page, fixture, baseURL }) => {
    await fixture.boot();
    await expect(linkOf(page)).toBeHidden(); await expect(goingOf(page)).toBeHidden();
    await expect(page.locator('[data-way="again"]')).toBeHidden(); await expect(page.locator('[data-way="reboot"]')).toBeHidden();
    await fixture.close();
    await fixture.pictures('waiting');
    await recoveryVisible(page); await unfinished(page);
    await expect(linkOf(page)).toHaveAttribute('href', `${baseURL.replace('127.0.0.1', '192.0.2.44')}/`);
    await expect(goingOf(page)).toHaveText('Following this machine to 192.0.2.44.');
    const requests = (await fixture.transport()).requests.length;
    await page.evaluate(() => window.__recovery.close(0));
    expect((await fixture.transport()).requests).toHaveLength(requests);
    await page.clock.fastForward(121000); await page.clock.runFor(1600);
    await recoveryVisible(page); await unfinished(page);
    await expect(goingOf(page)).toContainText('Nothing has answered at');
    await fixture.pictures('lost');
    expect(fixture.navigations).toEqual([]);
    await fixture.reconnect();
    await expect(linkOf(page)).toBeHidden(); await expect(goingOf(page)).toBeHidden();
    await expect(page.locator('[data-way="again"]')).toBeHidden(); await expect(page.locator('[data-way="reboot"]')).toBeHidden();
    await fixture.pictures('reconnected');
});

test('same-origin timeout is truthful and keeps exact relative href', async ({ page, fixture }) => {
    await fixture.boot({ address: null }); await fixture.close();
    await recoveryVisible(page); await expect(linkOf(page)).toHaveAttribute('href', '/');
    await page.clock.fastForward(121000); await page.clock.runFor(1600);
    await expect(goingOf(page)).toHaveText('The machine has not answered. Its own screen says where it is.');
    await unfinished(page); await fixture.pictures('same-origin-lost');
});

test('IPv6 preserves brackets and port; native Enter only reaches mocked destination', async ({ page, fixture, baseURL }) => {
    await fixture.boot({ address: '2001:db8::44/64' }); await fixture.close(); await recoveryVisible(page);
    const expected = `http://[2001:db8::44]:${new URL(baseURL).port}/`;
    await expect(linkOf(page)).toHaveAttribute('href', expected);
    expect((await fixture.transport()).commands).toEqual([]); expect(fixture.navigations).toEqual([]);
    fixture.destination(expected);
    await page.keyboard.press('Enter');
    await expect(page.getByRole('heading')).toHaveText('Mock destination reached');
    expect(fixture.navigations).toEqual([expected]);
});

test('reconnect before fetch settlement suppresses stale probes', async ({ page, fixture }) => {
    await fixture.boot(); await fixture.mode('pending'); await fixture.close();
    expect((await fixture.transport()).pending).toBe(2);
    await recoveryVisible(page); await fixture.reconnect();
    await page.evaluate(() => window.__recovery.resolvePending('opaque')); await page.clock.runFor(1000);
    await expect(linkOf(page)).toBeHidden(); await expect(goingOf(page)).toBeHidden();
    expect(fixture.navigations).toEqual([]);
});

test('existing setup responses do not cause automatic departure', async ({ page, fixture }) => {
    await fixture.boot({ address: null }); await fixture.mode('setup'); await fixture.close();
    await page.clock.runFor(5000); await recoveryVisible(page); expect(fixture.navigations).toEqual([]);
});

test('confirmed completion retains sign-in and finished state', async ({ page, fixture }) => {
    await fixture.boot(); await fixture.state({ ended: completed });
    await expect(page.locator('#turn')).toHaveClass(/finished/);
    await expect(page.locator('#turn h1')).toHaveText('Setup is complete');
    await expect(linkOf(page)).toBeVisible(); await expect(linkOf(page)).toHaveText('Sign in');
    await expect(page.locator('[data-way="again"]')).toBeHidden(); await expect(page.locator('[data-way="reboot"]')).toBeHidden();
    await fixture.close();
    await expect(page.locator('#status-text')).toHaveText('Setup has finished');
    await expect(goingOf(page)).toBeVisible(); await fixture.pictures('confirmed-completion');
});

test('opaque response retains the existing automatic arrival predicate', async ({ page, fixture, baseURL }) => {
    await fixture.boot();
    const destination = fixture.destination(`${baseURL.replace('127.0.0.1', '192.0.2.44')}/`);
    await fixture.mode('opaque'); await fixture.close(); await page.clock.runFor(1000);
    await expect(page.getByRole('heading')).toHaveText('Mock destination reached');
    expect(fixture.navigations).toEqual([destination]);
});

for (const outcome of ['failed', 'cancelled']) test(`${outcome} setup retains stop/start behavior`, async ({ page, fixture }) => {
    await fixture.boot({ ended: { outcome, message: `Fixture ${outcome}.` } });
    await expect(page.locator('#turn')).toHaveClass(/stopped/);
    await expect(linkOf(page)).toBeHidden(); await expect(goingOf(page)).toBeHidden();
    await expect(page.locator('[data-way="again"]')).toBeVisible();
    await expect(page.locator('[data-way="reboot"]')).toBeHidden();
    await fixture.close(); await page.clock.runFor(500);
    if (outcome === 'failed') {
        await expect(page.locator('#turn')).toHaveAttribute('data-kind', 'away');
        expect((await fixture.transport()).requests).toEqual([]);
    } else {
        await expect(linkOf(page)).toBeHidden(); await expect(goingOf(page)).toBeHidden();
        await expect(page.locator('[data-way="again"]')).toBeVisible();
    }
});

test('installer completion controls still send only fixture commands', async ({ page, fixture }) => {
    await fixture.boot({ job: 'install', ended: { outcome: 'complete', message: 'Installation complete. Reboot from the installed disk. When the machine restarts, remove the installation medium or choose the installed disk from the boot menu.', reboot: { ref: 'act.reboot', name: 'Reboot now' }, start: { ref: 'act.start', name: 'Back to the start' } } });
    await expect(linkOf(page)).toBeHidden();
    await expect(page.locator('[data-way="again"]')).toBeVisible();
    await expect(page.locator('[data-way="reboot"]')).toBeVisible();
    fixture.commands([{ press: 'act.reboot', seq: 6 }, { press: 'act.start', seq: 6 }]);
    await page.locator('[data-way="reboot"]').click(); await page.locator('[data-way="again"]').click();
});

test('ordinary installer disconnect retains away page', async ({ page, fixture }) => {
    await fixture.boot({ job: 'install' }); await fixture.close(); await page.clock.runFor(500);
    await expect(page.locator('#turn')).toHaveAttribute('data-kind', 'away');
    await expect(page.locator('#turn h1')).toHaveText('Lost touch with this machine');
    expect((await fixture.transport()).requests).toEqual([]);
});

test('ordinary-motion recovery remains pointer and keyboard usable', async ({ page, fixture }) => {
    await page.emulateMedia({ reducedMotion: 'no-preference' });
    await fixture.boot(); await fixture.close(); await recoveryVisible(page); await unfinished(page);
    await fixture.pictures('ordinary-motion-waiting');
});
