// ynote web — the browser adapter. Notes live in this browser's localStorage,
// automatically; there is no folder to open and nothing is uploaded. ALL
// markdown → HTML rendering is done by the Rust core compiled to WASM. Same
// rule as the desktop app: never render markdown in JavaScript.

import init, {
  render_fragment,
  render_standalone,
  preview_css,
  outline_json,
  doc_title,
} from "./vendor/ynote_wasm.js";
import { WELCOME_MD } from "./welcome.js";

const $ = (s) => document.querySelector(s);
const STORE_KEY = "ynote.notes.v1";

// Safe shortcuts: the browser reserves ⌘N/⌘T/⌘W and won't let a page capture
// them, so we use combos a page CAN preventDefault.
const isMac = navigator.platform.toLowerCase().includes("mac");
const MOD = isMac ? "⌘" : "Ctrl";
const KEY_HINT = {
  new: isMac ? "⌘⏎" : "Ctrl+Enter",
  save: `${MOD}S`,
  export: `${MOD}E`,
  view: `${MOD}\\`,
  sidebar: `${MOD}B`,
};

const state = {
  notes: [], // [{ id, content, created, updated }]
  current: null, // id
  content: "",
  view: "reading",
  editing: false, // actively typing inside the WYSIWYG preview
  renderTimer: null,
  saveTimer: null,
  normalizeTimer: null,
  toastTimer: null,
  _raf: 0,
};

// ------------------------------------------------------------------ boot ---

async function boot() {
  await init();
  $("#preview-style").textContent = preview_css();

  load();
  wireChrome();
  wireKeys();
  applyHints();
  setView("reading"); // default: work directly in the preview (WYSIWYG inline edit)

  if (!state.notes.length) seedWelcome();
  const first = [...state.notes].sort(byUpdatedDesc)[0];
  openNote(first.id);
}

// --------------------------------------------------------------- storage ---

function load() {
  try {
    state.notes = JSON.parse(localStorage.getItem(STORE_KEY)) || [];
  } catch {
    state.notes = [];
  }
}

function persist() {
  try {
    localStorage.setItem(STORE_KEY, JSON.stringify(state.notes));
  } catch (e) {
    toast("Couldn't save — browser storage is full");
  }
}

function seedWelcome() {
  const now = nowSec();
  state.notes = [{ id: newId(), content: WELCOME_MD, created: now, updated: now }];
  persist();
}

// ----------------------------------------------------------------- notes ---

function noteOf(id) {
  return state.notes.find((n) => n.id === id);
}
function byUpdatedDesc(a, b) {
  return (b.updated || 0) - (a.updated || 0);
}
function titleOf(note) {
  return (note && doc_title(note.content)) || "New note";
}

function openNote(id) {
  const note = noteOf(id);
  if (!note) return;
  flushSave();
  state.current = id;
  state.content = note.content;
  $("#editor").value = note.content;
  $("#active-path").textContent = titleOf(note);
  setSave("Saved", "saved");
  renderTree();
  renderPreview();
  updateStats();
}

function newNote() {
  const now = nowSec();
  const note = { id: newId(), content: "# New note\n\n", created: now, updated: now };
  state.notes.unshift(note);
  persist();
  openNote(note.id);
  focusNewNote();
}

function deleteNote(id) {
  const note = noteOf(id);
  if (!note) return;
  if (!confirm(`Delete “${titleOf(note)}”? This can't be undone.`)) return;
  state.notes = state.notes.filter((n) => n.id !== id);
  persist();
  if (!state.notes.length) seedWelcome();
  if (state.current === id) openNote([...state.notes].sort(byUpdatedDesc)[0].id);
  else renderTree();
}

// ----------------------------------------------------------------- tree ---

function renderTree() {
  const tree = $("#file-tree");
  tree.innerHTML = "";
  for (const note of [...state.notes].sort(byUpdatedDesc)) {
    tree.appendChild(noteRow(note));
  }
}

function noteRow(note) {
  const row = document.createElement("div");
  row.className = "file-row" + (note.id === state.current ? " active" : "");
  const sub = firstBodyLine(note.content) || "No additional text";
  row.innerHTML =
    `<div class="note-title">${escapeHtml(titleOf(note))}</div>` +
    `<div class="note-meta"><span class="note-date">${escapeHtml(
      formatDate(note.updated)
    )}</span><span class="note-sub">${escapeHtml(sub)}</span></div>`;
  row.onclick = () => openNote(note.id);
  row.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    if (confirmDeleteAllowed(e)) deleteNote(note.id);
  });
  return row;
}
// Right-click deletes (with its own confirm). Kept explicit so the handler
// reads clearly; no separate context menu to maintain for a single action.
function confirmDeleteAllowed() {
  return true;
}

// ---------------------------------------------------------------- editing ---

function onEdit() {
  const note = noteOf(state.current);
  if (!note) return;
  note.content = $("#editor").value;
  note.updated = nowSec();
  state.content = note.content;
  setSave("Editing…", "dirty");
  updateStats();
  clearTimeout(state.renderTimer);
  state.renderTimer = setTimeout(renderPreview, 100); // debounce 100ms
  scheduleSave();
}

function flushSave() {
  if (state.saveTimer) {
    clearTimeout(state.saveTimer);
    state.saveTimer = null;
  }
  persist();
  const note = noteOf(state.current);
  if (note) $("#active-path").textContent = titleOf(note);
  renderTree();
  setSave("Saved", "saved");
}

// --------------------------------------------------------------- preview ---

function renderPreview() {
  if (state.editing) return; // never clobber an in-progress WYSIWYG edit
  const md = $("#editor").value;
  const preview = $("#preview");
  const scroll = $("#preview-scroll").scrollTop;
  try {
    // Core wraps the fragment in <div class="ynote">…</div>; lift its inner
    // HTML so #preview.ynote drives styling — same technique as the desktop.
    const tmp = document.createElement("div");
    tmp.innerHTML = render_fragment(md);
    const inner = tmp.querySelector(".ynote");
    preview.innerHTML = (inner ? inner.innerHTML : tmp.innerHTML).trim();
    preview.contentEditable = "true"; // edit directly in the preview (WYSIWYG)
    $("#preview-scroll").scrollTop = scroll;
  } catch (e) {
    preview.innerHTML = `<p style="color:#d64545">Render error: ${escapeHtml(
      String(e.message || e)
    )}</p>`;
  }
  refreshOutline(md);
}

function refreshOutline(md) {
  let headings = [];
  try {
    headings = JSON.parse(outline_json(md));
  } catch {
    /* ignore */
  }
  const section = $("#outline-section");
  const list = $("#outline-list");
  list.innerHTML = "";
  section.hidden = !headings.length;
  for (const h of headings) {
    const item = document.createElement("div");
    item.className = "outline-item";
    item.dataset.level = h.level;
    item.textContent = h.text;
    item.onclick = () => jumpTo(h.slug);
    list.appendChild(item);
  }
}

function jumpTo(slug) {
  const el = $("#preview").querySelector(`#${cssEscape(slug)}`);
  if (el) el.scrollIntoView({ behavior: "smooth", block: "start" });
}

// ---------------------------------------------------------------- export ---

// The PDF engine (Typst compiled to WASM) is a large bundle, so it's loaded
// lazily — only the first time you export a PDF — to keep startup fast.
let pdfEnginePromise = null;
function loadPdfEngine() {
  if (!pdfEnginePromise) {
    pdfEnginePromise = import("./vendor-pdf/ynote_wasm.js").then(async (mod) => {
      await mod.default();
      return mod;
    });
  }
  return pdfEnginePromise;
}

async function doExport(fmt) {
  hideExportMenu();
  const note = noteOf(state.current);
  const title = titleOf(note);
  const md = $("#editor").value;
  const base = safeFileName(title);

  if (fmt === "html") {
    downloadBlob(render_standalone(md, title), base + ".html", "text/html");
    toast(`Downloaded “${base}.html”`);
    return;
  }

  if (fmt === "pdf") {
    try {
      toast("Preparing PDF engine…");
      const mod = await loadPdfEngine();
      const bytes = mod.export_pdf(md, true); // toc on
      downloadBlob(bytes, base + ".pdf", "application/pdf");
      toast(`Downloaded “${base}.pdf”`);
    } catch (e) {
      toast("PDF export failed: " + (e.message || e));
    }
    return;
  }

  // DOCX in the browser is the next port; desktop has it today.
  toast("DOCX export is only available in the desktop app");
}

function downloadBlob(text, name, mime) {
  const url = URL.createObjectURL(new Blob([text], { type: mime }));
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  a.click();
  URL.revokeObjectURL(url);
}

// ----------------------------------------------------------------- view ---

function setView(view) {
  state.view = view;
  $("#surface").className = "mode-" + view;
  document
    .querySelectorAll("#view-toggle button")
    .forEach((b) => b.classList.toggle("active", b.dataset.view === view));
}
function cycleView() {
  const order = ["source", "split", "reading"];
  setView(order[(order.indexOf(state.view) + 1) % order.length]);
}
function toggleSidebar() {
  $("#sidebar").classList.toggle("hidden");
}

function toggleExportMenu() {
  $("#export-menu").hidden = !$("#export-menu").hidden;
}
function hideExportMenu() {
  $("#export-menu").hidden = true;
}

// --------------------------------------------------------------- wiring ---

function wireChrome() {
  $("#editor").addEventListener("input", onEdit);
  setupPreviewEditing();
  setupDropImport();
  $("#btn-new").addEventListener("click", newNote);
  $("#btn-toggle-tree").addEventListener("click", toggleSidebar);
  $("#btn-export").addEventListener("click", (e) => {
    e.stopPropagation();
    toggleExportMenu();
  });
  document.querySelectorAll("#export-menu .export-opt").forEach((opt) =>
    opt.addEventListener("click", () => doExport(opt.dataset.fmt))
  );
  document.querySelectorAll("#view-toggle button").forEach((b) =>
    b.addEventListener("click", () => setView(b.dataset.view))
  );
  // Block toolbar (keep the caret in the contenteditable via mousedown guard).
  document.querySelectorAll("#format-bar button").forEach((b) => {
    b.addEventListener("mousedown", (e) => e.preventDefault());
    b.addEventListener("click", () => formatBlock(b.dataset.fmt));
  });
  // Click the empty area below the content to append a paragraph and keep going.
  $("#preview-scroll").addEventListener("mousedown", (e) => {
    const preview = $("#preview");
    if (preview.contentEditable !== "true") return;
    if (e.target !== preview && e.target !== $("#preview-scroll")) return;
    e.preventDefault();
    const last = preview.lastElementChild;
    let target;
    if (last && last.tagName === "P" && !last.textContent.trim()) {
      target = last;
    } else {
      target = document.createElement("p");
      target.appendChild(document.createElement("br"));
      preview.appendChild(target);
    }
    preview.focus();
    placeCaretEnd(target);
  });
  document.addEventListener("click", (e) => {
    if (!e.target.closest(".export-wrap")) hideExportMenu();
  });
}

function wireKeys() {
  window.addEventListener("keydown", (e) => {
    const mod = e.metaKey || e.ctrlKey;
    if (!mod) return;
    // ⌘/Ctrl + Enter → new note (⌘N is reserved by the browser).
    if (e.key === "Enter") { e.preventDefault(); newNote(); }
    else if (e.key === "s") { e.preventDefault(); flushSave(); }
    else if (e.key === "e") { e.preventDefault(); doExport("html"); }
    else if (e.key === "\\") { e.preventDefault(); cycleView(); }
    else if (e.key === "b") { e.preventDefault(); toggleSidebar(); }
  });
  // Autosave to localStorage is synchronous & instant, so there's no unsaved
  // work on unload — no beforeunload prompt needed.
}

function applyHints() {
  $("#btn-new").title = `New note (${KEY_HINT.new})`;
  $("#btn-export").title = `Export (${KEY_HINT.export})`;
  const seg = document.querySelectorAll("#view-toggle button");
  if (seg[1]) seg[1].title = `Split (${KEY_HINT.view} cycles)`;
  $("#btn-toggle-tree").title = `Toggle files (${KEY_HINT.sidebar})`;
}

// -------------------------------------------- edit directly in preview ---
//
// WYSIWYG (same logic as the desktop app): the whole preview is
// contenteditable. On each edit the rendered DOM is serialized back to
// markdown (front matter preserved verbatim) and saved. The preview is never
// re-rendered mid-edit, so edits can't be clobbered; a fresh render (which
// applies markdown formatting like "# " → heading) runs only when you leave
// the editor. The toolbar changes the current block's type.

function serializeInline(node) {
  let out = "";
  node.childNodes.forEach((n) => {
    if (n.nodeType === 3) {
      out += n.nodeValue;
      return;
    }
    if (n.nodeType !== 1) return;
    const t = n.tagName;
    if (t === "BR") out += "\n";
    else if (t === "STRONG" || t === "B") out += "**" + serializeInline(n) + "**";
    else if (t === "EM" || t === "I") out += "*" + serializeInline(n) + "*";
    else if (t === "DEL" || t === "S") out += "~~" + serializeInline(n) + "~~";
    else if (t === "CODE") out += "`" + n.textContent + "`";
    else if (t === "A") out += "[" + serializeInline(n) + "](" + (n.getAttribute("href") || "") + ")";
    else if (t === "IMG")
      out += "![" + (n.getAttribute("alt") || "") + "](" + (n.getAttribute("data-osrc") || n.getAttribute("src") || "") + ")";
    else if (t === "DIV") out += "\n" + serializeInline(n);
    else out += serializeInline(n);
  });
  return out;
}

function serializeList(listEl, depth) {
  const ordered = listEl.tagName === "OL";
  const pad = "  ".repeat(depth);
  let idx = ordered ? parseInt(listEl.getAttribute("start") || "1", 10) : 1;
  const lines = [];
  [...listEl.children].forEach((li) => {
    if (li.tagName !== "LI") return;
    const marker = ordered ? idx++ + "." : "-";
    let task = "";
    const cb = li.querySelector('input[type="checkbox"]');
    if (li.classList.contains("task-list-item") && cb) task = cb.checked ? "[x] " : "[ ] ";
    const clone = li.cloneNode(true);
    clone.querySelectorAll("ul,ol").forEach((n) => n.remove());
    const cbx = clone.querySelector('input[type="checkbox"]');
    if (cbx) cbx.remove();
    const text = serializeInline(clone).replace(/\s+/g, " ").trim();
    lines.push(pad + marker + " " + task + text);
    const nested = li.querySelector(":scope > ul, :scope > ol");
    if (nested) lines.push(serializeList(nested, depth + 1));
  });
  return lines.join("\n");
}

function serializeTable(t) {
  const heads = [...t.querySelectorAll("thead th")].map((th) =>
    serializeInline(th).replace(/\s+/g, " ").trim()
  );
  const aligns = [...t.querySelectorAll("thead th")].map((th) => {
    const s = th.getAttribute("style") || "";
    return s.includes("center") ? ":---:" : s.includes("right") ? "---:" : "---";
  });
  const rows = [...t.querySelectorAll("tbody tr")].map((tr) =>
    [...tr.children].map((td) => serializeInline(td).replace(/\s+/g, " ").trim())
  );
  const line = (arr) => "| " + arr.join(" | ") + " |";
  return [line(heads), line(aligns), ...rows.map(line)].join("\n");
}

function serializeBlock(el) {
  const tag = el.tagName;
  if (/^H[1-6]$/.test(tag))
    return "#".repeat(+tag[1]) + " " + serializeInline(el).replace(/\s+/g, " ").trim();
  if (tag === "P") return serializeInline(el).replace(/ /g, " ").trim();
  if (tag === "BLOCKQUOTE")
    return serializeInline(el).trim().split("\n").map((l) => "> " + l.trim()).join("\n");
  if (tag === "PRE") {
    const code = el.querySelector("code") || el;
    const m = (code.className || "").match(/language-([\w-]+)/);
    return "```" + (m ? m[1] : "") + "\n" + code.textContent.replace(/\n$/, "") + "\n```";
  }
  if (tag === "UL" || tag === "OL") return serializeList(el, 0);
  if (tag === "TABLE") return serializeTable(el);
  if (tag === "HR") return "---";
  return serializeInline(el).trim();
}

// Front matter of the current note, preserved verbatim (not shown in preview).
function docFrontMatter() {
  const m = state.content.match(/^﻿?---\n[\s\S]*?\n---\n?/);
  return m ? m[0] : "";
}

function serializeBody(container) {
  const parts = [];
  container.childNodes.forEach((node) => {
    if (node.nodeType === 3) {
      const t = node.nodeValue.replace(/\s+/g, " ").trim();
      if (t) parts.push(t);
      return;
    }
    if (node.nodeType !== 1) return;
    const md = serializeBlock(node);
    if (md.trim()) parts.push(md.trim());
  });
  const body = parts.join("\n\n").replace(/\n{3,}/g, "\n\n").trim();
  return body ? body + "\n" : "";
}

function syncFromPreview() {
  const preview = $("#preview");
  if (preview.contentEditable !== "true") return;
  const note = noteOf(state.current);
  if (!note) return;
  state.content = docFrontMatter() + serializeBody(preview);
  note.content = state.content;
  note.updated = nowSec();
  $("#editor").value = state.content; // keep the source view in sync (no input event)
  setSave("Editing…", "dirty");
  updateStats();
  scheduleSave();
}

function setupPreviewEditing() {
  const preview = $("#preview");
  preview.addEventListener("input", () => {
    state.editing = true;
    syncFromPreview();
  });
  // Backspace/Delete on an emptied block removes it (e.g. a heading you cleared).
  preview.addEventListener("keydown", (e) => {
    if (preview.contentEditable !== "true") return;
    if (e.key !== "Backspace" && e.key !== "Delete") return;
    const sel = window.getSelection();
    if (!sel || !sel.isCollapsed) return;
    const block = topBlock();
    if (!block || block.textContent.trim()) return;
    e.preventDefault();
    const neighbour =
      e.key === "Backspace" ? block.previousElementSibling : block.nextElementSibling;
    block.remove();
    state.editing = true;
    if (neighbour) placeCaretEnd(neighbour);
    else {
      preview.innerHTML = "";
      preview.focus();
    }
    syncFromPreview();
  });
  preview.addEventListener("focusout", (e) => {
    if (e.relatedTarget && preview.contains(e.relatedTarget)) return;
    state.editing = false;
    clearTimeout(state.normalizeTimer);
    state.normalizeTimer = setTimeout(() => {
      if (!state.editing) renderPreview();
    }, 200);
  });
}

function topBlock() {
  const preview = $("#preview");
  const sel = window.getSelection();
  if (!sel || !sel.rangeCount) return null;
  let n = sel.anchorNode;
  if (n && n.nodeType === 3) n = n.parentElement;
  if (!n || !preview.contains(n)) return null;
  while (n.parentElement && n.parentElement !== preview) n = n.parentElement;
  return n.parentElement === preview ? n : null;
}
function placeCaretEnd(el) {
  const r = document.createRange();
  r.selectNodeContents(el);
  r.collapse(false);
  const s = window.getSelection();
  s.removeAllRanges();
  s.addRange(r);
}
function formatBlock(fmt) {
  const preview = $("#preview");
  if (preview.contentEditable !== "true") return;
  const block = topBlock();
  const text = block ? block.textContent : "";
  let el;
  if (fmt === "ul" || fmt === "ol") {
    el = document.createElement(fmt);
    const li = document.createElement("li");
    li.textContent = text;
    el.appendChild(li);
  } else if (fmt === "pre") {
    el = document.createElement("pre");
    el.className = "mdcode";
    const code = document.createElement("code");
    code.textContent = text;
    el.appendChild(code);
  } else {
    el = document.createElement(fmt);
    el.textContent = text;
  }
  if (block) block.replaceWith(el);
  else preview.appendChild(el);
  placeCaretEnd(el.querySelector("li,code") || el);
  state.editing = true;
  syncFromPreview();
}

// After creating a note, select the title so typing replaces it.
function focusNewNote() {
  if (state.view !== "source") {
    const preview = $("#preview");
    preview.focus();
    const first = preview.firstElementChild;
    if (first) {
      const r = document.createRange();
      r.selectNodeContents(first);
      const s = window.getSelection();
      s.removeAllRanges();
      s.addRange(r);
    }
  } else {
    const ed = $("#editor");
    ed.focus();
    ed.setSelectionRange(0, ed.value.length);
  }
}

// -------------------------------------------------------- drag & drop import ---
//
// Drop .md files — or a whole folder of them — onto the app to import them as
// notes. Files are read in the browser and stored locally; nothing is uploaded.

function setupDropImport() {
  const zone = $("#app");
  const overlay = $("#drop-overlay");
  let depth = 0;
  const show = () => overlay.classList.add("on");
  const hide = () => overlay.classList.remove("on");

  ["dragenter", "dragover"].forEach((ev) =>
    zone.addEventListener(ev, (e) => {
      if (!e.dataTransfer || ![...e.dataTransfer.types].includes("Files")) return;
      e.preventDefault();
      depth = ev === "dragenter" ? depth + 1 : depth;
      show();
    })
  );
  zone.addEventListener("dragleave", () => {
    depth = Math.max(0, depth - 1);
    if (!depth) hide();
  });
  zone.addEventListener("drop", async (e) => {
    if (!e.dataTransfer || ![...e.dataTransfer.types].includes("Files")) return;
    e.preventDefault();
    depth = 0;
    hide();
    const files = await collectDropped(e.dataTransfer);
    await importFiles(files);
  });
}

// Gather File objects from a drop, recursing into any dropped directories.
async function collectDropped(dt) {
  const items = dt.items ? [...dt.items] : [];
  const entries = items.map((i) => (i.webkitGetAsEntry ? i.webkitGetAsEntry() : null)).filter(Boolean);
  if (!entries.length) return [...(dt.files || [])];
  const out = [];
  async function walk(entry) {
    if (entry.isFile) {
      const file = await new Promise((res) => entry.file(res));
      out.push(file);
    } else if (entry.isDirectory) {
      const reader = entry.createReader();
      const kids = await new Promise((res) => reader.readEntries(res));
      for (const k of kids) await walk(k);
    }
  }
  for (const e of entries) await walk(e);
  return out;
}

async function importFiles(files) {
  const md = files.filter((f) => /\.(md|mdx|markdown|txt)$/i.test(f.name));
  if (!md.length) {
    toast("Drop markdown (.md) files to import them");
    return;
  }
  let firstId = null;
  for (const f of md) {
    const text = await f.text();
    const created = f.lastModified ? Math.floor(f.lastModified / 1000) : nowSec();
    const note = { id: newId(), content: text, created, updated: nowSec() };
    state.notes.unshift(note);
    if (!firstId) firstId = note.id;
  }
  persist();
  openNote(firstId);
  toast(`Imported ${md.length} note${md.length > 1 ? "s" : ""}`);
}

// --------------------------------------------------------------- helpers ---

function scheduleSave() {
  clearTimeout(state.saveTimer);
  state.saveTimer = setTimeout(flushSave, 500);
}

function setSave(text, cls) {
  const el = $("#save-label");
  el.textContent = text;
  el.className = "save-label" + (cls ? " " + cls : "");
}

function updateStats() {
  const body = state.content.replace(/^---\n[\s\S]*?\n---\n?/, "");
  const words = (body.match(/\S+/g) || []).length;
  $("#stat-words").textContent = words + " words";
  $("#stat-read").textContent = Math.max(1, Math.round(words / 200)) + " min read";
}

function firstBodyLine(text) {
  const body = text.replace(/^---\n[\s\S]*?\n---\n?/, "");
  for (const raw of body.split("\n")) {
    const line = raw.replace(/^#+\s*/, "").trim();
    if (line) return line.replace(/[*_`>#\[\]]/g, "").slice(0, 80);
  }
  return "";
}

function formatDate(secs) {
  if (!secs) return "";
  const d = new Date(secs * 1000);
  const now = new Date();
  const day = 86400000;
  const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  const t = d.getTime();
  if (t >= startOfToday) return d.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  if (t >= startOfToday - day) return "Yesterday";
  if (t >= startOfToday - 6 * day) return d.toLocaleDateString([], { weekday: "long" });
  return d.getFullYear() === now.getFullYear()
    ? d.toLocaleDateString([], { month: "short", day: "numeric" })
    : d.toLocaleDateString([], { month: "numeric", day: "numeric", year: "2-digit" });
}

function toast(msg) {
  const t = $("#toast");
  $("#toast-text").textContent = msg;
  t.hidden = false;
  clearTimeout(state.toastTimer);
  state.toastTimer = setTimeout(() => (t.hidden = true), 2600);
}

function nowSec() {
  return Math.floor(Date.now() / 1000);
}
function newId() {
  return Date.now().toString(36) + Math.random().toString(36).slice(2, 7);
}
function safeFileName(s) {
  return ((s || "note").replace(/[/\\:*?"<>|]+/g, "-").replace(/\s+/g, " ").trim().slice(0, 80)) || "note";
}
function escapeHtml(s) {
  return String(s).replace(/[&<>]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" }[c]));
}
function cssEscape(s) {
  return window.CSS && CSS.escape ? CSS.escape(s) : s.replace(/[^a-zA-Z0-9_-]/g, "\\$&");
}

boot();
