# UI fixtures (factory F4 UI specialist)

The UI specialist (`apps/backend/src/factory/specialists.rs`, `UI`) builds a changed frontend app in a sandbox commands pod and screenshots the pages the change touches, so its review step (and the eval's judge) can look at the result. The pages render **offline**: every API call is answered from a fixture file that lives next to the app, and every other request leaving the page's origin is blocked. The code is in `apps/backend/src/automation/specialist_ui.rs` and the browser script in `apps/backend/src/automation/specialist_ui_shoot.js`.

This page documents that fixture file. Use it as the template for a new app. The admin app's file is [`apps/admin/.factory/ui-fixtures.json`](../../apps/admin/.factory/ui-fixtures.json).

## Where it lives

`<package>/.factory/ui-fixtures.json`, where `<package>` is the directory with the app's `package.json` (for the admin app, `apps/admin`). Agents never change it: any change under `.factory/` fails the specialist's `ui_paths_only` check.

Without the file the app is still installed, built, linted and tested, but nothing is screenshotted, and `ui_screenshots` is an advisory finding that names the missing file.

## Format

A JSON object. Unknown fields are an error, so a typo is caught instead of ignored. Both the top level and each fixture may carry a `description` string for humans; it is never used.

| Field | Required | Default | Meaning |
|---|---|---|---|
| `version` | no | `1` | Format version. Only `1` exists |
| `api_base` | yes | | Where the app's API lives: a string or a list of 1 to 8 strings. A string starting with `/` is a path prefix on the app's own origin (`"/v1/"`). An `http://` or `https://` string is an absolute URL prefix (`"https://api.example.com/v1/"`) |
| `fixtures` | no | `[]` | The API answers, at most 200; see below |
| `route_hints` | no | `{}` | Source glob → app route, at most 100. See [Routes](#routes) |
| `default_routes` | no | `["/"]` | Routes screenshotted when no route is named or hinted, at most 3 |
| `themes` | no | `["light"]` | `["light"]` or `["light", "dark"]`. Add `"dark"` only if the app really has a dark theme |
| `theme_storage` | no | none | `{"key", "light", "dark"}`: for an app that reads its theme from `localStorage`, the key and the value for each theme |
| `local_storage` | no | `{}` | `localStorage` entries set before the app loads (feature flags, a dismissed banner), at most 20 |

### Fixtures

Each entry answers the requests that match it:

| Field | Required | Default | Meaning |
|---|---|---|---|
| `method` | no | `"GET"` | `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `HEAD` or `OPTIONS`, any case |
| `path` | yes | | The URL path, without a query. It matches the path exactly; a trailing `*` makes it a prefix (`"/v1/tasks/*"`). `*` is allowed nowhere else |
| `query` | no | `{}` | Query parameters that must be present with exactly these values. Other parameters are ignored |
| `status` | no | `200` | The HTTP status, 100 to 599. A `204` or `304` has an empty body |
| `json` | no | `{}` | The response body, sent as `application/json`, at most 256 KiB |

The file is at most 1 MiB.

### Matching rules

For every request the page makes:

1. **Under `api_base`**: a request is under the API when its URL starts with an absolute `api_base`, or when it is on the app's origin and its path starts with a path `api_base`. The **first** fixture, in file order, whose method, `path` and `query` match answers it. Put specific entries before broad prefixes. A request under the API that no fixture matches gets `200` with `{}`.
2. **On the app's origin** (the built files): served by the app's own preview server.
3. **Anything else** (fonts, analytics, CDNs, other APIs): blocked. Nothing leaves the pod.

`{}` is a safe default for an object, not for a list: a page that calls `.map` on it throws, and a thrown error blocks the change. **Give every list endpoint a page calls on load a fixture**, even an empty `[]`.

## Routes

The pages screenshotted are, in order, with duplicates dropped and **at most 3**:

1. the routes the change names: the planning step's `routes` (and, in the frozen eval, the task's `routes`);
2. the routes whose `route_hints` glob matches a changed file, in the hints' alphabetical order. Globs are relative to the package: `*` stays within one path segment and `**` crosses segments (`"src/pages/Tasks.tsx": "/tasks"`, `"src/components/**": "/"`);
3. only when there are none, `default_routes`.

A route is a URL path on the app (`/tasks`, `/memories?tab=pinned`), never a full URL.

Each route is captured at a **390×844** phone and a **1280×800** desktop viewport, in the light theme, then in the dark one when `themes` has it. At most **6 images** per change: when the cap bites, every route keeps its light images and the dark ones are dropped. Each image is the page up to 2,400 pixels tall, as PNG, or JPEG when the PNG is over 850 KB.

**To add a route:** add a `route_hints` entry from the page's source file (or folder glob) to its route, and fixtures for every endpoint the page calls on load. If the page is behind a login, the session endpoint the app calls on boot must answer as a logged-in user (the admin app's is `/v1/admin/auth/me`).

## How a run uses it

In one commands pod, after the in-worker `ui_paths_only` check passed:

1. install from the lockfile (as for the tests specialist: `npm ci`, `corepack pnpm install --frozen-lockfile` or `corepack yarn install --frozen-lockfile`; `package-lock.json` wins when a directory has several);
2. the package's `typecheck` and `build` scripts (`ui_build`), its `lint` script (`ui_lint`), and its `test` script, or `vitest run` / `jest` when it depends on one but has no script (`ui_tests`);
3. after a passing build, the screenshot script: it serves the build with `vite preview` (a package that depends on `vite`) or `next start` (one that depends on `next`) on `127.0.0.1`, waits up to 60 s for it to answer, and opens each page with the image's bundled Playwright Chromium. It waits for the page to load and its network to settle (at most 8 s), then captures it. The whole script has 170 s.

The images come back on the pod's stdout, one framed base64 line each. The worker keeps only planned names whose bytes are a real PNG or JPEG under the size cap, writes them into a scratch directory of the review's checkout (`.nm-ui-review/`) after the change's files and diff were taken, tells the review to open each one, and deletes the directory when the review ends. They never reach a pull request.

## Advisory or blocking

| Finding | Effect |
|---|---|
| Install, `typecheck`, `build`, `lint` or tests fail | **Blocking** (`ui_build`, `ui_lint`, `ui_tests`) |
| A page throws an uncaught error | **Blocking** (`ui_screenshots`) |
| A page renders an empty body (no text, no image, no control) | **Blocking** |
| The screenshot script fails after the fixture file exists, or Playwright is missing from the image | **Blocking**: the change cannot be shown, so it is not accepted |
| No `build`, `lint` or `test` script | Advisory: listed as skipped |
| No fixture file, or an invalid one (the finding names the problem) | Advisory |
| The app is neither Vite nor Next.js | Advisory |
| The app could not be served, a route could not be loaded, a page redirected (for example to `/login`), an image is missing or over the cap, or the time ran out | Advisory |

The review sees every advisory finding and judges those pages from the code instead. A blocking finding stops the change before the review.
