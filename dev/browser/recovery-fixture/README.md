# Recovery UI regression fixture

SDK-free real-DOM checks for PEI1411. The server binds **only 127.0.0.1** and serves the checkout's actual `page/` HTML, modules, stylesheet and fonts. It applies the first-boot HTML transformation from `src/page.rs` and reads that source file's exact installer/setup CSP strings. No production module is substituted or rewritten.

## Run

From this directory, with Node 22 or newer:

```sh
npm ci --ignore-scripts --no-audit --no-fund
npm run test:fixture
npx --no-install playwright install --with-deps chromium
npm run test:baseline
npm test
```

`@playwright/test` and its lockfile pin 1.57.0. Normally Playwright uses its matching browser build. An already-installed Chromium can be selected explicitly with `RECOVERY_CHROMIUM=/absolute/path/to/chromium`; reports record the actual browser version. `RECOVERY_PORT` defaults to 7797. Do not point the fixture at an installer or expose its listener publicly.

The complementary zero-dependency state test runs from the repository root:

```sh
node --experimental-vm-modules dev/browser/recovery-state.mjs
```

## Boundaries and controls

- Browser-init mocks replace WebSocket and fetch before application startup. Tests drive the production socket handlers and capture commands. No socket connects to a daemon. Fetch returns only controlled synthetic failure, setup, pending or opaque results. A failure's underlying cause is deliberately unspecified.
- A default-deny browser request route permits only loopback fixture assets. Designated native/automatic destination navigations receive an in-browser marker response. Documentation-only IPv4/IPv6 destinations are never contacted.
- Playwright's clock is installed before the document loads. It advances the existing two-minute patience deadline without waiting in real time. Reduced-motion projects use the production intro's own skip behavior; separate ordinary-motion checks use its Skip intro control.
- No CSS, browser certificate exceptions, CSP, login, network security policy, production polling interval, or navigation predicate is changed. The fixture has no installation or reboot implementation.

## Coverage and evidence

The candidate suite runs the same 12 cases at 1280×800 and 390×844, each with dark and light browser preferences (48 cases). The application supports **one dark design**; the light-preference runs check that design under a different browser preference and do not claim a light theme.

Checks include:

- Waiting and timed-out unfinished setup: effectively visible neutral “Open this machine” link and truthful status, unchanged heading, 66% overall progress and incomplete network phase, no Start again/Reboot, and no unsolicited navigation/command.
- Visibility through ancestor styles, nonzero layout, opacity, viewport containment, native keyboard tab reachability, and pointer hit-testing/actionability.
- Reconnect hides recovery again; repeated close does not create a second polling round. The pending-response test specifically covers reconnect **before fetch settlement**, not every asynchronous race in the existing controller.
- Same-origin href, changed IPv4, IPv6 brackets and explicit port. Native Enter navigation reaches only a mocked exact destination. HTTPS href construction is covered by the separate Node state fixture; this browser fixture makes no HTTPS/trust claim.
- Confirmed-complete sign-in and finished styling, failed/cancelled stop/start behavior, ordinary installer away state, and preserved install reboot/start command wiring with commands captured entirely in memory.
- Existing setup-response and opaque-arrival predicates remain intact.

Full-page and focused recovery screenshots, JSON source hashes/state/transport evidence, browser version, JSON/HTML test reports, failure traces and logs live under `results/`. Main visual states have screenshots across all four viewport/preference combinations. Screenshots are evidence for inspection; these tests do not use pixel-golden matching.

`baseline.mjs` extracts only `page/` and `src/page.rs` from exact commit `71fb803a2c1717d2fbc9fe95a85571231810eca1`, leaving the working tree untouched. It runs the same `@recovery-regression` visibility assertion in all four projects. Success means exactly four failures at the expected hidden-link assertion, with no infrastructure or other assertion errors. It writes `results/baseline/verification.json`. A browser-launch failure is never accepted as baseline red.

The pull-request workflow uses read-only repository permissions, SHA-pinned official actions, checkout credentials not persisted, no secrets, no `pull_request_target`, and no repository-writing step. It uploads evidence even when checks fail. It needs no Peios SDK or sibling Rust checkouts and does not run the daemon-dependent `dev/browser/finish.mjs`.

These checks establish mocked UI rendering/interaction only. They do not establish real installation success, network migration, HTTPS reachability, certificate trust, CORS/CSP transport behavior, server authorization, authentication, or end-to-end sign-in.

Official references: [clock](https://playwright.dev/docs/clock), [preloaded browser API mocks](https://playwright.dev/docs/mock-browser-apis), [GitHub Actions CI](https://playwright.dev/docs/ci).
