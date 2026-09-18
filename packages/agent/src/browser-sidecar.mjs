// Browser sidecar: a minimal Chrome DevTools Protocol driver.
//
// The Rust gateway launches this fixed process and talks to it over JSON
// lines (stdin/stdout). It connects to the user's Chrome/Edge via
// playwright-core in headless mode with a throwaway profile, so it never
// touches the user's real browsing session or history.
//
// Commands (one JSON object per line):
//   {"type":"open","requestId":"r","url":"https://..."}
//   {"type":"snapshot","requestId":"r"}
//   {"type":"click","requestId":"r","ref":"e3"}
//   {"type":"type","requestId":"r","ref":"e3","text":"..."}
//   {"type":"select","requestId":"r","ref":"e3","value":"..."}
//   {"type":"wait","requestId":"r"}
//   {"type":"extract_text","requestId":"r"}
//   {"type":"close","requestId":"r"}
//   {"type":"status","requestId":"r"}
//
// Every command answers exactly one result line:
//   {"type":"result","requestId":"r","ok":true,"text":"...","details":{...}}
//   {"type":"result","requestId":"r","ok":false,"text":"..."}

import readline from "node:readline";
import { existsSync } from "node:fs";
import { chromium } from "playwright-core";

const CHROME_CANDIDATES = [
  process.env.PHOEBE_CHROME_PATH,
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
  "/Applications/Chromium.app/Contents/MacOS/Chromium",
  "/usr/bin/google-chrome",
  "/usr/bin/chromium",
  "/usr/bin/chromium-browser",
  "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
  "C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe",
  "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
  "C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe",
].filter(Boolean);

const NAV_TIMEOUT_MS = 30_000;
const MAX_SNAPSHOT_ITEMS = 200;
const SNAPSHOT_SELECTOR =
  'a, button, input, select, textarea, [role="button"], [role="link"], [role="checkbox"], [role="radio"], [contenteditable="true"]';

let browser = null;
let context = null;
let page = null;

function send(event) {
  process.stdout.write(JSON.stringify(event) + "\n");
}

function result(requestId, ok, text, details = null) {
  const event = { type: "result", requestId, ok, text };
  if (details !== null) event.details = details;
  send(event);
}

function findChrome() {
  for (const candidate of CHROME_CANDIDATES) {
    if (candidate && existsSync(candidate)) return candidate;
  }
  return null;
}

async function ensureBrowser() {
  if (browser && browser.isConnected()) return;
  const executablePath = findChrome();
  if (!executablePath) {
    throw new Error("未找到 Chrome / Edge 浏览器；请安装后重试");
  }
  browser = await chromium.launch({
    executablePath,
    headless: true,
    args: ["--no-first-run", "--no-default-browser-check", "--disable-features=Translate"],
  });
  browser.on("disconnected", () => {
    browser = null;
    context = null;
    page = null;
  });
  context = await browser.newContext({
    userAgent:
      "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
    viewport: { width: 1280, height: 900 },
    locale: "zh-CN",
  });
  page = await context.newPage();
}

async function requirePage() {
  await ensureBrowser();
  if (!page) throw new Error("浏览器页面未就绪");
  return page;
}

function bounded(value, limit = 2000) {
  const text = String(value ?? "").trim();
  return text.length > limit ? `${text.slice(0, limit)}…（已截断）` : text;
}

async function openUrl(url) {
  const target = await requirePage();
  await target.goto(url, { waitUntil: "domcontentloaded", timeout: NAV_TIMEOUT_MS });
  return {
    text: `已打开 ${target.url()}`,
    details: { url: target.url(), title: await target.title() },
  };
}

async function snapshot() {
  const target = await requirePage();
  const data = await target.evaluate(({ selector, maxItems }) => {
    const items = [];
    const nodes = document.querySelectorAll(selector);
    for (let index = 0; index < nodes.length && items.length < maxItems; index++) {
      const node = nodes[index];
      const ref = `e${index}`;
      node.setAttribute("data-phoebe-ref", ref);
      const tag = node.tagName.toLowerCase();
      const role = node.getAttribute("role") || "";
      const inputType = node.getAttribute("type") || "";
      const label =
        node.getAttribute("aria-label") ||
        node.getAttribute("placeholder") ||
        node.getAttribute("name") ||
        (tag === "a" || tag === "button" ? node.textContent : node.value) ||
        "";
      const text = String(label).replace(/\s+/g, " ").trim();
      if (!text && !role && !inputType) continue;
      items.push({
        ref,
        tag,
        role,
        type: inputType,
        text: text.length > 80 ? `${text.slice(0, 80)}…` : text,
      });
    }
    return {
      url: location.href,
      title: document.title,
      bodyTextLength: (document.body?.innerText || "").length,
      items,
      truncated: nodes.length > maxItems,
    };
  }, { selector: SNAPSHOT_SELECTOR, maxItems: MAX_SNAPSHOT_ITEMS });
  return {
    text: `页面快照：${data.title || data.url}（${data.items.length} 个可交互元素${data.truncated ? "，已截断" : ""}）`,
    details: data,
  };
}

async function elementByRef(target, ref) {
  if (typeof ref !== "string" || !/^e[0-9]{1,6}$/.test(ref)) {
    throw new Error("元素引用无效；请先 snapshot 获取最新 ref");
  }
  const locator = target.locator(`[data-phoebe-ref="${ref}"]`);
  const count = await locator.count();
  if (count === 0) throw new Error(`找不到元素 ${ref}；页面可能已变化，请重新 snapshot`);
  return locator.first();
}

async function click(ref) {
  const target = await requirePage();
  const element = await elementByRef(target, ref);
  await element.click({ timeout: 10_000 });
  await target.waitForLoadState("domcontentloaded", { timeout: 10_000 }).catch(() => {});
  return { text: `已点击元素 ${ref}`, details: { ref, url: target.url() } };
}

async function type(ref, text) {
  const target = await requirePage();
  const element = await elementByRef(target, ref);
  await element.click({ timeout: 10_000 });
  await element.fill(String(text ?? ""), { timeout: 10_000 });
  return { text: `已向元素 ${ref} 输入`, details: { ref } };
}

async function select(ref, value) {
  const target = await requirePage();
  const element = await elementByRef(target, ref);
  await element.selectOption(String(value ?? ""), { timeout: 10_000 });
  return { text: `已选择 ${ref}`, details: { ref, value } };
}

async function waitPage() {
  const target = await requirePage();
  await target.waitForLoadState("networkidle", { timeout: 20_000 }).catch(() => {});
  return { text: `页面已稳定：${target.url()}`, details: { url: target.url() } };
}

async function extractText() {
  const target = await requirePage();
  const text = await target.evaluate(() => (document.body ? document.body.innerText : ""));
  return { text: bounded(text, 12_000), details: { url: target.url(), title: await target.title() } };
}

async function closeBrowser() {
  if (browser) {
    await browser.close().catch(() => {});
  }
  browser = null;
  context = null;
  page = null;
  return { text: "浏览器已关闭" };
}

async function handle(command) {
  const ok = (r) => result(command.requestId, true, r.text, r.details);
  switch (command.type) {
    case "status":
      return result(command.requestId, true, browser && browser.isConnected() ? "ready" : "idle");
    case "open":
      return ok(await openUrl(command.url));
    case "snapshot":
      return ok(await snapshot());
    case "click":
      return ok(await click(command.ref));
    case "type":
      return ok(await type(command.ref, command.text));
    case "select":
      return ok(await select(command.ref, command.value));
    case "wait":
      return ok(await waitPage());
    case "extract_text":
      return ok(await extractText());
    case "close":
      return ok(await closeBrowser());
    default:
      return result(command.requestId, false, "未知的浏览器命令");
  }
}

const lines = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
let queue = Promise.resolve();
lines.on("line", (line) => {
  let command;
  try {
    command = JSON.parse(line);
  } catch {
    send({ type: "result", requestId: null, ok: false, text: "无效命令" });
    return;
  }
  if (!command || typeof command !== "object" || !command.requestId || !command.type) {
    send({ type: "result", requestId: null, ok: false, text: "无效命令" });
    return;
  }
  // Process commands strictly in order: a click must wait for the open/snapshot
  // that produced its element ref, and close must wait for everything else.
  queue = queue
    .then(() => handle(command))
    .catch((error) => result(command.requestId, false, bounded(error?.message || error, 400)));
});
lines.on("close", async () => {
  // Drain any in-flight commands before exiting so tests and clean shutdown
  // still see every result.
  await queue.catch(() => {});
  if (browser) await browser.close().catch(() => {});
  process.exit(0);
});
