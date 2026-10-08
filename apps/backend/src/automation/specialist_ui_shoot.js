'use strict'
// Screenshots for the factory UI specialist (see automation/specialist_ui.rs and
// docs/factory/ui-fixtures.md). Runs in a sandbox commands pod, never in the
// worker, after the app was built:
//
//   node shoot.js <config.json>
//
// It serves the build (`vite preview` or `next start`, from the config), then
// opens each planned page in the bundled Chromium with every API call answered
// from the fixture file and every other off-origin request blocked, so the
// pages render offline and the same way on every run.
//
// Output is stdout only, one record per line, so the worker can parse it
// without trusting anything else the pod prints:
//
//   NMUI-SHOT <name.png|name.jpg> <base64>
//   NMUI-RESULT <json>              (exactly once, last)
//
// Known conditions (the app could not be served, a page threw) are reported in
// the result with exit code 0. Any other exit means the script itself failed.

const fs = require('fs')
const http = require('http')
const { spawn } = require('child_process')

const config = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'))
const started = Date.now()
const deadline = started + config.budget_ms
const left = () => Math.max(0, deadline - Date.now())
const origin = `http://127.0.0.1:${config.port}`
const result = { served: false, serve_detail: null, fatal: null, pages: [], skipped: [] }
let server = null

const write = line => new Promise(resolve => process.stdout.write(line + '\n', () => resolve()))
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms))
const short = (text, max) => String(text == null ? '' : text).slice(0, max)

function stopServer() {
  if (!server) return
  try {
    process.kill(-server.pid, 'SIGTERM')
  } catch (_) {
    // Already gone.
  }
  server = null
}

async function finish(code) {
  stopServer()
  await write('NMUI-RESULT ' + JSON.stringify(result))
  process.exit(code)
}

// A backstop for a hung browser: the budget bounds every wait below, so this
// only fires when something ignores its own timeout.
setTimeout(() => {
  result.fatal = 'script_timeout'
  finish(1)
}, config.budget_ms + 30000).unref()

function logTail() {
  try {
    const text = fs.readFileSync(config.server_log, 'utf8')
    return short(text.slice(-600), 600)
  } catch (_) {
    return ''
  }
}

function probe() {
  return new Promise(resolve => {
    const request = http.get(origin + '/', response => {
      response.resume()
      resolve(true)
    })
    request.on('error', () => resolve(false))
    request.setTimeout(2000, () => {
      request.destroy()
      resolve(false)
    })
  })
}

async function serve() {
  const log = fs.openSync(config.server_log, 'a')
  server = spawn(config.server[0], config.server.slice(1), {
    cwd: process.cwd(),
    detached: true,
    stdio: ['ignore', log, log],
    env: { ...process.env, PORT: String(config.port), BROWSER: 'none', NODE_ENV: 'production' },
  })
  let exited = null
  server.on('exit', code => {
    exited = code
  })
  server.on('error', error => {
    exited = short(error.message, 200)
  })
  const until = Date.now() + Math.min(config.serve_timeout_ms, left())
  while (Date.now() < until) {
    if (exited !== null) {
      result.serve_detail = `the server exited (${exited}): ${logTail()}`
      return false
    }
    if (await probe()) return true
    await sleep(400)
  }
  result.serve_detail = `nothing answered on port ${config.port} within ${config.serve_timeout_ms} ms: ${logTail()}`
  return false
}

const fixtures = config.fixtures

function underApi(url) {
  return fixtures.api_base.some(base =>
    base.startsWith('/') ? url.origin === origin && url.pathname.startsWith(base) : url.href.startsWith(base),
  )
}

function pathMatches(pattern, pathname) {
  return pattern.endsWith('*') ? pathname.startsWith(pattern.slice(0, -1)) : pathname === pattern
}

function findFixture(method, url) {
  return fixtures.fixtures.find(
    fixture =>
      fixture.method === method &&
      pathMatches(fixture.path, url.pathname) &&
      Object.entries(fixture.query || {}).every(([key, value]) => url.searchParams.get(key) === value),
  )
}

async function answer(route) {
  const request = route.request()
  let url
  try {
    url = new URL(request.url())
  } catch (_) {
    return route.abort('blockedbyclient')
  }
  if (underApi(url)) {
    const fixture = findFixture(request.method().toUpperCase(), url)
    const status = fixture ? fixture.status : 200
    const empty = status === 204 || status === 304
    return route.fulfill({
      status,
      contentType: 'application/json',
      body: empty ? '' : JSON.stringify(fixture ? fixture.json : {}),
    })
  }
  if (url.origin === origin) return route.continue()
  // Fonts, analytics, CDNs: nothing leaves the pod, and nothing is waited on.
  return route.abort('blockedbyclient')
}

function initScript(storage) {
  try {
    for (const [key, value] of Object.entries(storage)) window.localStorage.setItem(key, value)
  } catch (_) {
    // Storage may be unavailable on an error page.
  }
}

async function shoot(browser, shot) {
  const page = { name: shot.name, route: shot.route, status: 'ok', final_path: null, errors: [], console_errors: 0 }
  const storage = { ...(fixtures.local_storage || {}) }
  if (fixtures.theme_storage) storage[fixtures.theme_storage.key] = fixtures.theme_storage[shot.theme]
  const context = await browser.newContext({
    viewport: { width: shot.width, height: shot.height },
    deviceScaleFactor: 1,
    colorScheme: shot.theme,
    reducedMotion: 'reduce',
    serviceWorkers: 'block',
  })
  try {
    await context.addInitScript(initScript, storage)
    await context.route('**/*', answer)
    const tab = await context.newPage()
    tab.on('pageerror', error => {
      if (page.errors.length < 5) page.errors.push(short(error && error.message ? error.message : error, 300))
    })
    tab.on('console', message => {
      if (message.type() === 'error') page.console_errors += 1
    })
    let response = null
    try {
      response = await tab.goto(origin + shot.route, { waitUntil: 'load', timeout: Math.min(30000, left()) })
    } catch (error) {
      page.status = 'unreachable'
      page.errors.push(short(error.message, 300))
      return page
    }
    if (response && response.status() >= 400) {
      page.status = 'unreachable'
      page.errors.push(`HTTP ${response.status()}`)
      return page
    }
    try {
      await tab.waitForLoadState('networkidle', { timeout: Math.min(8000, left()) })
    } catch (_) {
      // A page that keeps polling is still screenshotted.
    }
    await tab.waitForTimeout(Math.min(700, left()))
    page.final_path = new URL(tab.url()).pathname
    const blank = await tab.evaluate(() => {
      const body = document.body
      if (!body) return true
      const text = (body.innerText || '').trim()
      const visible = body.querySelectorAll('img,svg,canvas,video,input,button,select,textarea').length
      return text.length === 0 && visible === 0
    })
    if (page.errors.length > 0) page.status = 'page_error'
    else if (blank) page.status = 'blank'
    const height = await tab.evaluate(() => document.documentElement.scrollHeight)
    const clip = { x: 0, y: 0, width: shot.width, height: Math.max(shot.height, Math.min(height, config.max_height)) }
    let image = await tab.screenshot({ type: 'png', fullPage: true, clip, timeout: Math.min(20000, left()) })
    let name = shot.name + '.png'
    for (const quality of [70, 45]) {
      if (image.length <= config.max_image_bytes) break
      image = await tab.screenshot({ type: 'jpeg', quality, fullPage: true, clip, timeout: Math.min(20000, left()) })
      name = shot.name + '.jpg'
    }
    if (image.length > config.max_image_bytes) {
      result.skipped.push({ name: shot.name, reason: `image over ${config.max_image_bytes} bytes` })
    } else {
      await write(`NMUI-SHOT ${name} ${image.toString('base64')}`)
    }
    return page
  } finally {
    await context.close().catch(() => {})
  }
}

async function main() {
  let chromium
  try {
    ;({ chromium } = require('playwright'))
  } catch (error) {
    result.fatal = 'playwright_unavailable: ' + short(error.message, 200)
    return finish(0)
  }
  if (!(await serve())) return finish(0)
  result.served = true
  // A pod's /dev/shm is small: Chromium crashes on large pages unless it uses /tmp.
  const browser = await chromium.launch({
    headless: true,
    args: ['--no-sandbox', '--disable-dev-shm-usage'],
    timeout: Math.min(60000, left()),
  })
  try {
    for (const shot of config.shots) {
      if (left() < 15000) {
        result.skipped.push({ name: shot.name, reason: 'time budget spent' })
        continue
      }
      result.pages.push(await shoot(browser, shot))
    }
  } finally {
    await browser.close().catch(() => {})
  }
  return finish(0)
}

main().catch(error => {
  result.fatal = 'script_error: ' + short(error && error.message ? error.message : error, 300)
  finish(1)
})
