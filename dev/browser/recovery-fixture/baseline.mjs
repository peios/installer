// The same visibility assertion must fail on the exact unmodified baseline.
// Infrastructure failures, missing reports and unexpected failures never count as red.
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
const here = fileURLToPath(new URL('.', import.meta.url));
const repository = fileURLToPath(new URL('../../../', import.meta.url));
const baseline = '71fb803a2c1717d2fbc9fe95a85571231810eca1';
const root = join(here, 'results/baseline/source');
// Remove the dedicated generated directory, including stale reports and source bytes.
rmSync(join(here, 'results/baseline'), { recursive: true, force: true });
const actual = execFileSync('git', ['rev-parse', `${baseline}^{commit}`], { cwd: repository, encoding: 'utf8' }).trim();
if (actual !== baseline) throw new Error('Baseline commit did not resolve exactly');
const files = execFileSync('git', ['ls-tree', '-rz', '--name-only', baseline, 'page', 'src/page.rs'], { cwd: repository, encoding: 'utf8' }).split('\0').filter(Boolean);
for (const file of files) {
    const target = join(root, file); mkdirSync(dirname(target), { recursive: true });
    writeFileSync(target, execFileSync('git', ['show', `${baseline}:${file}`], { cwd: repository, maxBuffer: 8 * 1024 * 1024 }));
}
const run = spawnSync(process.execPath, [join(here, 'node_modules/@playwright/test/cli.js'), 'test', '--config=playwright.config.mjs', '--grep=@recovery-regression'], {
    cwd: here, stdio: 'inherit', env: { ...process.env, SOURCE_ROOT: join(root, 'page'), SOURCE_COMMIT: baseline, RECOVERY_RUN: 'baseline' },
});
if (run.error) throw run.error;
const report = JSON.parse(readFileSync(join(here, 'results/baseline/report.json'), 'utf8'));
const specs = suites => suites.flatMap(suite => [...suite.specs, ...specs(suite.suites || [])]);
const tests = specs(report.suites).flatMap(spec => spec.tests);
const correct = run.status === 1 && !report.errors.length && tests.length === 4 && tests.every(test =>
    test.results.length === 1 && test.results[0].status === 'failed' && test.results[0].errors.length === 1 &&
    test.results[0].errors[0].message.includes('PEI1411 recovery link is effectively hidden'));
const summary = { baseline, expectedRegressionFailures: 4, observedTests: tests.length, expectedRedVerified: correct, childExit: run.status };
writeFileSync(join(here, 'results/baseline/verification.json'), `${JSON.stringify(summary, null, 2)}\n`);
console.log(JSON.stringify(summary, null, 2));
if (!correct) throw new Error('Baseline did not fail solely on the intended recovery-visibility assertion');
