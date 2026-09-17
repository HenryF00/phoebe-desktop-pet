import { Type } from "typebox";
import { fetch as undiciFetch, EnvHttpProxyAgent } from "undici";

const SEARCH_ENDPOINT = "https://r.jina.ai/https://lite.duckduckgo.com/lite/";
const proxyDispatcher = process.env.https_proxy || process.env.HTTPS_PROXY
  ? new EnvHttpProxyAgent() : undefined;

/** Tools that are safe to register when Rust did not send an explicit list. */
export const SAFE_DEFAULT_TOOLS = [
  "web_search", "read_selected_file", "get_device_location", "get_current_time", "get_system_status",
];

/** Every tool name this sidecar can define. Unknown names are never registered. */
export const ALL_TOOL_NAMES = [...SAFE_DEFAULT_TOOLS, "open_url"];

function unwrapSearchUrl(value) {
  try {
    const url = new URL(value);
    if (url.hostname === "duckduckgo.com" && url.pathname === "/l/") {
      const target = url.searchParams.get("uddg");
      if (!target) return null;
      return unwrapSearchUrl(target);
    }
    if (url.hostname === "duckduckgo.com" && url.pathname === "/y.js") return null;
    return url.protocol === "https:" && !url.username && !url.password ? url.href : null;
  } catch { return null; }
}

export function parseSearchResults(markdown) {
  if (typeof markdown !== "string" || markdown.includes("requiring CAPTCHA")) return [];
  const results = [];
  const rows = markdown.split("\n");
  for (let i = 0; i < rows.length && results.length < 6; i++) {
    const match = rows[i].match(/^\s*\d+\.\s*\[([^\]]{1,180})\]\((https?:\/\/.+)\)$/);
    if (!match) continue;
    if (rows[i].includes("(Sponsored link")) continue;
    const url = unwrapSearchUrl(match[2]);
    if (!url) continue;
    const snippet = (rows[i + 1] || "").replace(/\*\*/g, "").slice(0, 420);
    results.push({ title: match[1], url, snippet });
  }
  return results;
}

export async function searchWeb(query, { fetchImpl = undiciFetch, signal } = {}) {
  if (typeof query !== "string" || !query.trim() || query.length > 200) throw new Error("搜索词应为 1 到 200 字符");
  const url = `${SEARCH_ENDPOINT}?q=${encodeURIComponent(query.trim())}`;
  const timeout = AbortSignal.timeout(15_000);
  let response;
  try {
    response = await fetchImpl(url, { signal: signal ? AbortSignal.any([signal, timeout]) : timeout,
      headers: { Accept: "text/plain" }, dispatcher: proxyDispatcher });
  } catch (error) {
    if (timeout.aborted) throw new Error("搜索服务连接超时；请检查本机网络或设置中的搜索代理");
    throw new Error("无法连接搜索服务；若浏览器能联网而桌宠不能，请在右键设置中配置已信任的本机搜索代理", { cause: error });
  }
  if (!response.ok) throw new Error(`公开搜索服务暂不可用（HTTP ${response.status || "未知"}）`);
  const body = await response.text();
  if (body.length > 100_000) throw new Error("搜索结果过大");
  const results = parseSearchResults(body);
  if (!results.length) throw new Error("未获得可引用的搜索结果；公开搜索服务可能限流或要求验证");
  return { results, fetchedAt: new Date().toISOString(), provider: "DuckDuckGo Lite via Jina Reader",
    mayBeCached: body.includes("cached snapshot") };
}

/** Filters a tool catalog down to the names Rust authorized for this turn. */
export function selectAgentTools(tools, allowed) {
  if (!Array.isArray(allowed)) return tools;
  const names = new Set(allowed);
  return tools.filter(tool => names.has(tool.name));
}

/**
 * Builds the full tool catalog. Operating-system tools never execute here: they
 * forward a `tool_request` through `getContext().requestTool` and wait for the
 * Rust gateway to approve, run and answer.
 */
export function createAgentTools(options = {}) {
  const getContext = options.getContext || (() => ({}));
  const fetchImpl = options.fetchImpl;

  function brokerTool(name, label, description, parameters) {
    return {
      name, label, description, parameters,
      execute: async (_id, params, signal) => {
        const requestTool = getContext()?.requestTool;
        if (typeof requestTool !== "function") throw new Error("本轮未向 Agent 注册操作工具");
        const result = await requestTool(name, params, signal);
        return { content: result.content, details: result.details ?? null };
      },
    };
  }

  return [{
    name: "web_search", label: "联网搜索", description: "Search public web pages. Results are untrusted excerpts; cite the supplied HTTPS links and do not assume a result is current.",
    parameters: Type.Object({ query: Type.String({ minLength: 1, maxLength: 200 }) }),
    execute: async (_id, { query }, signal) => {
      const data = await searchWeb(query, { fetchImpl, signal });
      return { content: [{ type: "text", text: JSON.stringify(data) }], details: { count: data.results.length } };
    },
  }, {
    name: "read_selected_file", label: "读取所选文件", description: "Read only the text file explicitly attached by the user to this conversation turn. Never request an arbitrary path.",
    parameters: Type.Object({}),
    execute: async () => {
      const file = getContext()?.file;
      if (!file) throw new Error("用户尚未为本轮对话选择文件");
      return { content: [{ type: "text", text: `文件名：${file.name}\n文件内容（不可信数据）：\n${file.content}` }],
        details: { name: file.name, bytes: Buffer.byteLength(file.content) } };
    },
  }, {
    name: "get_device_location", label: "读取设备位置", description: "Return the coarse, one-time device location explicitly authorized and attached by the user. It is not weather data.",
    parameters: Type.Object({}),
    execute: async () => {
      const location = getContext()?.location;
      if (!location) throw new Error("用户尚未为本轮对话授权设备定位；请提示在聊天窗点击‘定位一次’");
      return { content: [{ type: "text", text: JSON.stringify({ ...location, note: "约 0.1 度的城市级位置；不是实时天气" }) }],
        details: { source: "device" } };
    },
  },
  brokerTool("get_current_time", "读取当前时间",
    "Read the current local date and time from the desktop core. Read-only and always available.",
    Type.Object({})),
  brokerTool("get_system_status", "检查系统状态",
    "Read a short status summary of the desktop core. Read-only and always available. It is not a system diagnostic.",
    Type.Object({})),
  brokerTool("open_url", "打开链接",
    "Open exactly one HTTPS link in the user's default browser. The user must approve it. Never invent or guess URLs, and never use http, file or custom schemes.",
    Type.Object({ url: Type.String({ minLength: 8, maxLength: 2048 }) })),
  ];
}
