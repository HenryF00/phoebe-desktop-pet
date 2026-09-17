import React from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

type ExplicitMemory = { id: string; title: string; content: string; created_at: string; updated_at: string };
const inTauri = "__TAURI_INTERNALS__" in window;

export function MemorySection({ onDirtyChange, resetToken }: { onDirtyChange: (dirty: boolean) => void; resetToken: number }) {
  const [items, setItems] = React.useState<ExplicitMemory[]>([]);
  const [selectedId, setSelectedId] = React.useState<string | null>(null);
  const [title, setTitle] = React.useState("");
  const [content, setContent] = React.useState("");
  const [loading, setLoading] = React.useState(inTauri);
  const [busy, setBusy] = React.useState(false);
  const [notice, setNotice] = React.useState("");
  const [error, setError] = React.useState("");
  const titleRef = React.useRef<HTMLInputElement>(null);
  const selected = items.find(item => item.id === selectedId);
  const dirty = selected ? title !== selected.title || content !== selected.content : Boolean(title || content);

  React.useEffect(() => { onDirtyChange(dirty); }, [dirty, onDirtyChange]);
  React.useEffect(() => { setSelectedId(null); setTitle(""); setContent(""); setError(""); }, [resetToken]);
  React.useEffect(() => {
    if (!inTauri) return;
    let disposed = false;
    let unlisten: (() => void) | null = null;
    const refresh = async () => {
      setLoading(true);
      try {
        const next = await invoke<ExplicitMemory[]>("get_explicit_memories");
        if (!disposed) { setItems(next); setNotice(""); }
      } catch { if (!disposed) setNotice("本机记忆暂不可读取；请检查应用数据目录权限。"); }
      finally { if (!disposed) setLoading(false); }
    };
    void refresh();
    void listen<boolean>("settings-visibility", event => { if (event.payload) void refresh(); })
      .then(fn => { if (disposed) fn(); else unlisten = fn; });
    return () => { disposed = true; unlisten?.(); };
  }, []);

  function choose(item: ExplicitMemory | null) {
    if (dirty && !window.confirm("记忆草稿尚未保存，仍要切换吗？")) return;
    setSelectedId(item?.id ?? null); setTitle(item?.title ?? ""); setContent(item?.content ?? "");
    setError(""); setNotice(""); titleRef.current?.focus();
  }

  async function save(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!inTauri || busy || loading || !title.trim() || !content.trim()) return;
    const bytes = new TextEncoder();
    if (bytes.encode(title.trim()).length > 120 || bytes.encode(content.trim()).length > 600) {
      setError("标题最多 120 字节、内容最多 600 字节；请缩短后重试。"); titleRef.current?.focus(); return;
    }
    setBusy(true); setError(""); setNotice("");
    try {
      const confirmed = await invoke<boolean>("save_explicit_memory", { id: selectedId, title, content });
      if (!confirmed) { setNotice("已取消；长期记忆未更改。"); return; }
      setItems(await invoke<ExplicitMemory[]>("get_explicit_memories"));
      setSelectedId(null); setTitle(""); setContent("");
      setNotice(selectedId ? "长期记忆已更新。" : "长期记忆已保存。");
    } catch (reason) { setError(typeof reason === "string" ? reason : "保存失败；请检查内容或应用数据目录权限后重试。"); }
    finally { setBusy(false); }
  }

  async function remove() {
    if (!inTauri || busy || !selectedId) return;
    setBusy(true); setError(""); setNotice("");
    try {
      const confirmed = await invoke<boolean>("delete_explicit_memory", { id: selectedId });
      if (!confirmed) { setNotice("已取消；长期记忆未更改。"); return; }
      setItems(await invoke<ExplicitMemory[]>("get_explicit_memories"));
      setSelectedId(null); setTitle(""); setContent(""); setNotice("长期记忆已删除。");
    } catch (reason) { setError(typeof reason === "string" ? reason : "删除失败；请刷新列表后重试。"); }
    finally { setBusy(false); }
  }

  return <fieldset className="settings-group memory-group"><legend>长期记忆</legend>
    <p className="field-help">仅保存你明确填写的偏好，不从聊天或模型推断自动写入。密码、凭证、支付和身份信息不可保存；隐私模式不会自动删除已有记忆。</p>
    {loading ? <p role="status">正在读取本机记忆…</p> : items.length === 0 ? <p className="memory-empty">尚无长期记忆。填写下方内容即可新增。</p>
      : <ul className="memory-list" aria-label="已保存的长期记忆">{items.map(item => <li key={item.id}>
        <button type="button" className={item.id === selectedId ? "is-selected" : ""} onClick={() => choose(item)} aria-pressed={item.id === selectedId}>
          <strong>{item.title}</strong><span>{item.content}</span>
        </button></li>)}</ul>}
    <form className="memory-form" onSubmit={save}>
      <div className="memory-form-heading"><strong>{selectedId ? "编辑所选记忆" : "新增记忆"}</strong>{selectedId && <button type="button" onClick={() => choose(null)} disabled={busy}>取消编辑</button>}</div>
      <label htmlFor="memory-title">偏好标题</label>
      <input id="memory-title" ref={titleRef} value={title} onChange={event => setTitle(event.target.value)} disabled={!inTauri || loading || busy} placeholder="例如：回复语言" aria-invalid={Boolean(error)} />
      <label htmlFor="memory-content">具体内容</label>
      <textarea id="memory-content" value={content} onChange={event => setContent(event.target.value)} disabled={!inTauri || loading || busy} placeholder="例如：默认使用简体中文回复" aria-invalid={Boolean(error)} />
      <p className="field-help">每次新增、修改或删除都需要桌面核心弹出原生确认。</p>
      {error && <p className="memory-error" role="alert">{error}</p>}
      {notice && <p className="memory-notice" role="status" aria-live="polite">{notice}</p>}
      <div className="memory-actions"><button type="submit" disabled={!inTauri || loading || busy || !dirty || !title.trim() || !content.trim()}>{busy ? "处理中…" : selectedId ? "保存修改" : "保存记忆"}</button>
        {selectedId && <button type="button" className="memory-delete" onClick={() => void remove()} disabled={!inTauri || busy}>删除所选</button>}</div>
    </form>
  </fieldset>;
}
