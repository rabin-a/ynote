// papery desktop frontend — "field-notes" reskin over the Tauri command
// surface. No bundler; uses the global Tauri API (withGlobalTauri).
// All markdown → HTML rendering happens in Rust core (render_preview), never here.

const { invoke } = window.__TAURI__.core;
const dialog = window.__TAURI__.dialog;
const opener = window.__TAURI__.opener;

const $ = (s) => document.querySelector(s);

const state = {
  root: null,
  file: null,
  content: "",
  docs: [],
  dirty: new Set(),
  headingLines: {},
  view: "split",
  renaming: false, // suppress tree rebuilds while an inline rename is active
  newMode: "file", // "file" | "group" for the inline new-item input
  editing: false, // actively typing in the WYSIWYG preview
  renderTimer: null,
  saveTimer: null,
  toastTimer: null,
};

// ---------------------------------------------------------------- project ---

async function loadProject(root) {
  try {
    const info = await invoke("open_project", { path: root });
    state.root = info.root;
    state.docs = info.docs;
    state.dirty.clear();
    $("#project-name").textContent = info.name;
    const css = await invoke("preview_css", { root: state.root });
    $("#preview-style").textContent = css;
    renderTree();
    setSave("Ready", "");
    if (info.docs.length) await openFile(info.docs[0].path);
    else clearSurface();
  } catch (e) {
    setSave("Error: " + e, "dirty");
  }
}

function dotClass(path) {
  if (path.endsWith(".toml")) return "dot-toml";
  if (path.endsWith(".md")) return "dot-md";
  return "dot-other";
}

function entryOf(path) {
  return state.docs.find((d) => d.path === path);
}
// Newest first (Apple Notes style). Config/untitled with no created time sink.
function byCreatedDesc(a, b) {
  return (entryOf(b)?.created || 0) - (entryOf(a)?.created || 0);
}
// The list label is the note's *name* — its title (front matter / heading /
// first line). Filenames are never shown; empty notes read "New note".
function labelOf(path) {
  if (path === "papery.toml") return "Project settings";
  const e = entryOf(path);
  return e && e.title ? e.title : "New note";
}

// Apple Notes-style date: time today, "Yesterday", weekday this week, else date.
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

function groupDocs() {
  const all = state.docs.map((d) => d.path).slice();
  if (!all.includes("papery.toml")) all.push("papery.toml");
  const roots = all.filter((p) => !p.includes("/")).sort(byCreatedDesc);
  const folders = {};
  all
    .filter((p) => p.includes("/"))
    .forEach((p) => {
      const f = p.split("/")[0];
      (folders[f] = folders[f] || []).push(p);
    });
  const groups = [{ folder: null, files: roots }];
  Object.keys(folders)
    .sort()
    .forEach((f) => groups.push({ folder: f, files: folders[f].sort(byCreatedDesc) }));
  return groups;
}

function renderTree() {
  // Don't rebuild the tree while an inline rename input is open — a rebuild
  // would destroy the input (openFile's re-render was clobbering renames).
  if (state.renaming) return;
  const tree = $("#file-tree");
  tree.innerHTML = "";
  for (const g of groupDocs()) {
    if (g.folder) {
      const h = document.createElement("div");
      h.className = "folder-head";
      h.innerHTML = `<span class="chev">▼</span>${escapeHtml(g.folder)}`;
      makeDropTarget(h, g.folder); // drop a file here to move it into this group
      h.addEventListener("contextmenu", (e) => {
        e.preventDefault();
        showCtxMenu(e.clientX, e.clientY, [
          { label: "Delete group", danger: true, fn: () => deleteGroupAction(g.folder) },
        ]);
      });
      tree.appendChild(h);
    }
    for (const path of g.files) {
      const row = document.createElement("div");
      row.className = "file-row" + (path === state.file ? " active" : "");
      const isConfig = path === "papery.toml";
      const e = entryOf(path);
      const dirty = state.dirty.has(path) ? '<span class="note-dirty"></span>' : "";
      if (isConfig) {
        row.innerHTML = `<div class="note-title">${escapeHtml(labelOf(path))}</div>`;
      } else {
        // Apple Notes card: title, then date · subtitle.
        const date = formatDate(e && e.created);
        const sub = e && e.subtitle ? e.subtitle : "No additional text";
        row.innerHTML =
          `<div class="note-title">${escapeHtml(labelOf(path))}${dirty}</div>` +
          `<div class="note-meta"><span class="note-date">${escapeHtml(date)}</span>` +
          `<span class="note-sub">${escapeHtml(sub)}</span></div>`;
      }
      row.onclick = () => openFile(path);
      if (!isConfig) {
        row.draggable = true;
        row.addEventListener("dragstart", (ev) => {
          ev.dataTransfer.setData("text/plain", path);
          ev.dataTransfer.effectAllowed = "move";
        });
        const titleEl = row.querySelector(".note-title");
        titleEl.ondblclick = (ev) => {
          ev.stopPropagation();
          beginRename(path, titleEl);
        };
        row.addEventListener("contextmenu", (ev) => {
          ev.preventDefault();
          showCtxMenu(ev.clientX, ev.clientY, [
            { label: "Rename", fn: () => beginRename(path, row.querySelector(".note-title")) },
            { label: "Delete", danger: true, fn: () => deleteFileAction(path) },
          ]);
        });
      }
      tree.appendChild(row);
    }
  }
}

// ----------------------------------------------------------------- editing ---

async function openFile(path) {
  if (state.file && state.dirty.has(state.file)) await save();
  try {
    let content = "";
    try {
      content = await invoke("read_file", { root: state.root, path });
    } catch {
      content = ""; // e.g. papery.toml that doesn't exist yet
    }
    state.file = path;
    state.content = content;
    $("#editor").value = content;
    updateActiveLabel();
    setSave("Saved", "saved");
    renderTree();
    await renderPreview();
    updateStats();
  } catch (e) {
    setSave("Error opening " + path + ": " + e, "dirty");
  }
}

function clearSurface() {
  state.file = null;
  state.content = "";
  $("#editor").value = "";
  $("#active-path").textContent = "—";
  $("#preview").innerHTML = "";
  $("#outline-section").hidden = true;
}

function onEdit() {
  state.content = $("#editor").value;
  if (state.file) state.dirty.add(state.file);
  setSave("Editing…", "dirty");
  renderTree();
  updateStats();
  clearTimeout(state.renderTimer);
  state.renderTimer = setTimeout(renderPreview, 100); // debounce 100ms
  clearTimeout(state.saveTimer);
  state.saveTimer = setTimeout(save, 900); // autosave shortly after you stop
}

async function renderPreview() {
  if (!state.root || !state.file) return;
  if (state.editing) return; // never clobber an in-progress WYSIWYG edit
  const isMd = state.file.endsWith(".md");
  const preview = $("#preview");
  if (!isMd) {
    preview.contentEditable = "false";
    preview.innerHTML = `<pre class="mdcode"><code>${escapeHtml(state.content)}</code></pre>`;
    $("#outline-section").hidden = true;
    return;
  }
  try {
    const scroll = $("#preview-scroll").scrollTop;
    const html = await invoke("render_preview", {
      root: state.root,
      path: state.file,
      content: state.content,
    });
    const tmp = document.createElement("div");
    tmp.innerHTML = html;
    const inner = tmp.querySelector(".papery");
    preview.innerHTML = (inner ? inner.innerHTML : html).trim(); // trim so :empty shows placeholder
    preview.contentEditable = "true"; // edit directly in the preview
    $("#preview-scroll").scrollTop = scroll;
    await refreshOutline();
    updateActiveHeading();
  } catch (e) {
    preview.innerHTML = `<pre style="color:#c0503f">${escapeHtml(String(e))}</pre>`;
  }
}

async function refreshOutline() {
  const section = $("#outline-section");
  const list = $("#outline-list");
  try {
    const headings = await invoke("get_outline", { content: state.content });
    state.headingLines = {};
    list.innerHTML = "";
    for (const h of headings) {
      state.headingLines[h.slug] = h.line;
      const item = document.createElement("div");
      item.className = "outline-item";
      item.dataset.level = h.level;
      item.dataset.slug = h.slug;
      item.textContent = h.text;
      item.onclick = () => jumpTo(h.slug);
      list.appendChild(item);
    }
    section.hidden = headings.length === 0;
  } catch {
    section.hidden = true;
  }
}

function jumpTo(slug) {
  const target = $("#preview").querySelector(`[id="${cssEscape(slug)}"]`);
  if (target) target.scrollIntoView({ behavior: "smooth", block: "start" });
  const line = state.headingLines[slug];
  if (line != null) {
    const ed = $("#editor");
    const lines = state.content.split("\n");
    let off = 0;
    for (let i = 0; i < line - 1 && i < lines.length; i++) off += lines[i].length + 1;
    ed.setSelectionRange(off, off);
    ed.scrollTop = ((line - 1) / Math.max(1, lines.length)) * ed.scrollHeight;
  }
}

function updateActiveHeading() {
  const scroller = $("#preview-scroll");
  const rect = scroller.getBoundingClientRect();
  let active = null;
  $("#preview")
    .querySelectorAll("h1[id],h2[id],h3[id],h4[id],h5[id],h6[id]")
    .forEach((h) => {
      if (h.getBoundingClientRect().top - rect.top <= 96) active = h.id;
    });
  document.querySelectorAll(".outline-item").forEach((it) => {
    it.classList.toggle("active", it.dataset.slug === active);
  });
}

// ------------------------------------------------------------------ saving ---

async function save() {
  if (!state.file || !state.dirty.has(state.file) || !state.root) return;
  try {
    await invoke("write_file", { root: state.root, path: state.file, content: state.content });
    state.dirty.delete(state.file);
    // Refresh titles (derived from content) so the list reflects the edit.
    state.docs = await invoke("list_docs", { root: state.root });
    renderTree();
    updateActiveLabel();
    setSave("Saved", "saved");
  } catch (e) {
    setSave("Save failed: " + e, "dirty");
  }
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

// The tab bar shows the note's name (title), never its filename.
function updateActiveLabel() {
  $("#active-path").textContent = state.file ? labelOf(state.file) : "—";
}

// ------------------------------------------------------------------ export ---

// Turn a note title into a safe default filename for export.
function safeFileName(s) {
  const name = (s || "note").replace(/[/\\:*?"<>|]+/g, "-").replace(/\s+/g, " ").trim();
  return name.slice(0, 80) || "note";
}

async function doExport(fmt) {
  hideExportMenu();
  if (!state.root || !state.file) return;
  const base = safeFileName(labelOf(state.file)); // export uses the note's name, not the filename
  // Ask the user where to save.
  let dest;
  try {
    dest = await dialog.save({
      title: `Export as ${fmt.toUpperCase()}`,
      defaultPath: `${base}.${fmt}`,
      filters: [{ name: fmt.toUpperCase(), extensions: [fmt] }],
    });
  } catch (e) {
    setSave("Export failed: " + e, "dirty");
    return;
  }
  if (!dest) return; // cancelled
  try {
    if (state.dirty.has(state.file)) await save();
    setSave("Exporting…", "");
    const outPath = await invoke("export_doc", {
      root: state.root,
      path: state.file,
      format: fmt,
      out: dest,
      toc: $("#toc-toggle").checked,
    });
    showToast(`Exported ${outPath.split("/").pop()}`);
    setSave("Saved", "saved");
    try {
      await opener.revealItemInDir(outPath);
    } catch {
      /* reveal is best-effort */
    }
  } catch (e) {
    setSave("Export failed: " + e, "dirty");
  }
}

function showToast(text) {
  $("#toast-text").textContent = text;
  $("#toast").hidden = false;
  clearTimeout(state.toastTimer);
  state.toastTimer = setTimeout(() => ($("#toast").hidden = true), 2600);
}

function toggleExportMenu() {
  $("#export-menu").hidden = !$("#export-menu").hidden;
}
function hideExportMenu() {
  $("#export-menu").hidden = true;
}

// -------------------------------------------------- new file / new group ---

// A date-stamped filename like 2026-07-05-1430.md — unique and easy to find on
// disk (the list still shows the note's title, derived from its first line).
function dateStamp() {
  const d = new Date();
  const p = (n) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}-${p(d.getHours())}${p(d.getMinutes())}`;
}
function uniqueUntitled(folder) {
  const prefix = folder ? folder.replace(/\/+$/, "") + "/" : "";
  const existing = new Set(state.docs.map((d) => d.path));
  const base = dateStamp();
  let name = `${prefix}${base}.md`;
  let n = 1;
  while (existing.has(name)) {
    name = `${prefix}${base}-${n}.md`;
    n++;
  }
  return name;
}

// A simple starter so a new note isn't a blank page — the user just edits it.
const NEW_NOTE_TITLE = "New note";
const NEW_NOTE_TEMPLATE = `# ${NEW_NOTE_TITLE}\n\nStart writing here…\n`;

// Focus the active editing surface: the preview (WYSIWYG) in reading mode,
// otherwise the source textarea.
function focusEditSurface() {
  if (state.view === "reading") {
    const p = $("#preview");
    p.focus();
    placeCaretEnd(p);
  } else {
    $("#editor").focus();
  }
}

// After creating a note, select the title so typing immediately replaces it.
function focusNewNote() {
  if (state.view === "reading") {
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
    const i = state.content.indexOf(NEW_NOTE_TITLE);
    if (i >= 0) ed.setSelectionRange(i, i + NEW_NOTE_TITLE.length);
  }
}

async function newUntitledFile(folder) {
  if (!state.root) return;
  const name = uniqueUntitled(folder);
  try {
    // Seed a simple starter note so it's ready to edit, not a blank page.
    await invoke("write_file", { root: state.root, path: name, content: NEW_NOTE_TEMPLATE });
    state.docs = await invoke("list_docs", { root: state.root });
    renderTree();
    await openFile(name);
    focusNewNote();
  } catch (e) {
    setSave("Create failed: " + e, "dirty");
  }
}

function showNew(mode) {
  state.newMode = mode;
  const box = $("#new-file");
  box.hidden = false;
  const input = $("#new-name");
  input.placeholder = mode === "group" ? "new group name" : "note name (.md added)";
  const hint = box.querySelector(".new-hint");
  if (hint) {
    hint.textContent =
      mode === "group" ? "↵ create group · esc cancel" : "↵ create .md file · esc cancel";
  }
  input.value = "";
  input.focus();
}
function hideNew() {
  $("#new-file").hidden = true;
}
async function commitNew() {
  const val = $("#new-name").value.trim();
  hideNew();
  if (!val) return;
  if (state.newMode === "group") {
    await newUntitledFile(val.replace(/^\/+|\/+$/g, ""));
  } else {
    await newNamedFile(val);
  }
}

// Add a markdown file directly by name (blank canvas; `.md` appended if omitted).
async function newNamedFile(name) {
  if (!state.root) return;
  name = forceMd(name.replace(/^\/+/, "")); // always a .md file
  if (state.docs.some((d) => d.path === name)) {
    await openFile(name);
    return;
  }
  try {
    await invoke("write_file", { root: state.root, path: name, content: "" });
    state.docs = await invoke("list_docs", { root: state.root });
    renderTree();
    await openFile(name);
    focusEditSurface();
  } catch (e) {
    setSave("Create failed: " + e, "dirty");
  }
}

// ------------------------------------------------------------ rename file ---

// Renaming a note edits its *title* (the note's name), which lives in the
// content's first line / heading — the filename never changes.
function beginRename(path, nameEl) {
  if (path === "papery.toml") return;
  state.renaming = true;
  const current = labelOf(path) === "New note" ? "" : labelOf(path);
  const input = document.createElement("input");
  input.className = "rename-input";
  input.value = current;
  input.title = "Rename note";
  input.spellcheck = false;
  nameEl.replaceWith(input);
  input.focus();
  input.select();

  let done = false;
  const finish = (commit) => {
    if (done) return;
    done = true;
    state.renaming = false;
    if (commit) commitTitle(path, input.value);
    else renderTree();
  };
  input.onclick = (e) => e.stopPropagation();
  input.onkeydown = (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      finish(true);
    } else if (e.key === "Escape") {
      e.preventDefault();
      finish(false);
    }
  };
  input.onblur = () => finish(true);
}

// Set a document's title in its markdown: front matter `title`, else the first
// heading's text, else the first non-empty line (preserving heading level).
function setDocTitle(content, title) {
  const bom = content.startsWith("﻿") ? "﻿" : "";
  const rest = content.slice(bom.length);
  const fm = rest.match(/^---\n[\s\S]*?\n---\n?/);
  if (fm && /^title:/m.test(fm[0])) {
    return bom + rest.replace(/^title:.*$/m, "title: " + title);
  }
  const fmStr = fm ? fm[0] : "";
  const lines = rest.slice(fmStr.length).split("\n");
  let i = 0;
  while (i < lines.length && !lines[i].trim()) i++;
  if (i < lines.length) {
    const h = lines[i].match(/^(#{1,6}\s+)/);
    lines[i] = (h ? h[1] : "") + title;
  } else {
    lines.unshift(title);
  }
  return bom + fmStr + lines.join("\n");
}

async function commitTitle(path, value) {
  const title = value.trim();
  if (!title || title === labelOf(path)) return renderTree();
  try {
    const content =
      state.file === path
        ? state.content
        : await invoke("read_file", { root: state.root, path });
    const updated = setDocTitle(content, title);
    await invoke("write_file", { root: state.root, path, content: updated });
    if (state.file === path) {
      state.content = updated;
      $("#editor").value = updated;
      state.dirty.delete(path);
      if (!state.editing) await renderPreview();
    }
    state.docs = await invoke("list_docs", { root: state.root });
    renderTree();
    updateActiveLabel();
    setSave("Renamed", "saved");
  } catch (e) {
    setSave("Rename failed: " + e, "dirty");
    renderTree();
  }
}

// Make an element a drop target that moves the dropped file into `folder`
// (null = project root).
function makeDropTarget(el, folder) {
  el.addEventListener("dragover", (e) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = "move";
    el.classList.add("drop-target");
  });
  el.addEventListener("dragleave", () => el.classList.remove("drop-target"));
  el.addEventListener("drop", (e) => {
    e.preventDefault();
    el.classList.remove("drop-target");
    moveFile(e.dataTransfer.getData("text/plain"), folder);
  });
}

async function moveFile(path, folder) {
  if (!path || path === "papery.toml") return;
  const base = path.split("/").pop();
  const to = folder ? `${folder}/${base}` : base;
  if (to === path) return;
  if (state.docs.some((d) => d.path === to)) {
    setSave("A note with that name already exists in that group", "dirty");
    return;
  }
  try {
    if (state.file === path && state.dirty.has(path)) await save();
    await invoke("rename_file", { root: state.root, from: path, to });
    if (state.file === path) state.file = to;
    state.dirty.delete(path);
    state.docs = await invoke("list_docs", { root: state.root });
    renderTree();
    updateActiveLabel();
    setSave("Moved to " + (folder || "Files"), "saved");
  } catch (e) {
    setSave("Move failed: " + e, "dirty");
  }
}

// ----------------------------------------------- context menu / deleting ---

function showCtxMenu(x, y, items) {
  const m = $("#ctx-menu");
  m.innerHTML = "";
  items.forEach((it) => {
    const b = document.createElement("div");
    b.className = "ctx-item" + (it.danger ? " danger" : "");
    b.textContent = it.label;
    b.onclick = () => {
      hideCtxMenu();
      it.fn();
    };
    m.appendChild(b);
  });
  m.hidden = false;
  m.style.left = Math.min(x, window.innerWidth - m.offsetWidth - 8) + "px";
  m.style.top = Math.min(y, window.innerHeight - m.offsetHeight - 8) + "px";
}
function hideCtxMenu() {
  $("#ctx-menu").hidden = true;
}

async function deleteFileAction(path) {
  if (path === "papery.toml") return;
  const ok = await dialog.confirm(`Delete “${labelOf(path)}”? This can’t be undone.`, {
    title: "Delete note",
    kind: "warning",
  });
  if (!ok) return;
  try {
    await invoke("delete_file", { root: state.root, path });
    state.dirty.delete(path);
    state.docs = await invoke("list_docs", { root: state.root });
    if (state.file === path) {
      if (state.docs.length) await openFile(state.docs[0].path);
      else clearSurface();
    }
    renderTree();
    showToast("Deleted");
  } catch (e) {
    setSave("Delete failed: " + e, "dirty");
  }
}

async function deleteGroupAction(folder) {
  const n = state.docs.filter((d) => d.path.startsWith(folder + "/")).length;
  const ok = await dialog.confirm(
    `Delete group “${folder}” and its ${n} note${n === 1 ? "" : "s"}? This can’t be undone.`,
    { title: "Delete group", kind: "warning" }
  );
  if (!ok) return;
  try {
    await invoke("delete_group", { root: state.root, folder });
    state.docs = await invoke("list_docs", { root: state.root });
    if (state.file && state.file.startsWith(folder + "/")) {
      if (state.docs.length) await openFile(state.docs[0].path);
      else clearSurface();
    }
    renderTree();
    showToast("Deleted group " + folder);
  } catch (e) {
    setSave("Delete failed: " + e, "dirty");
  }
}

// -------------------------------------------------------------- view/keys ---

function setView(view) {
  state.view = view;
  $("#surface").className = "mode-" + view;
  document.querySelectorAll("#view-toggle button").forEach((b) =>
    b.classList.toggle("active", b.dataset.view === view)
  );
}
function cycleView() {
  const order = ["source", "split", "reading"];
  setView(order[(order.indexOf(state.view) + 1) % order.length]);
}
function toggleSidebar() {
  $("#sidebar").classList.toggle("hidden");
}

// ------------------------------------------------------------------ utils ---

function escapeHtml(s) {
  return String(s).replace(/[&<>]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" }[c]));
}
// papery only manages markdown files: strip any typed extension and force `.md`.
function forceMd(name) {
  return name.replace(/\.[A-Za-z][\w]*$/, "") + ".md";
}
function cssEscape(s) {
  return window.CSS && CSS.escape ? CSS.escape(s) : s.replace(/"/g, '\\"');
}

// ------------------------------------------------ edit directly in preview ---
//
// WYSIWYG: the whole preview is contenteditable. On each edit the rendered DOM
// is serialized back to markdown (front matter preserved verbatim) and saved.
// The preview is never re-rendered mid-edit, so edits can't be overridden; a
// fresh render (which applies markdown formatting like "# " → heading) runs
// only when you leave the editor. The toolbar changes the current block's type.

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
    else if (t === "DIV") out += "\n" + serializeInline(n); // contenteditable line breaks
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
  if (tag === "P") return serializeInline(el).replace(/ /g, " ").trim();
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

// Front matter of the current doc, preserved verbatim (not shown in preview).
function docFrontMatter() {
  const m = state.content.match(/^﻿?---\n[\s\S]*?\n---\n?/);
  return m ? m[0] : "";
}

// Serialize the whole editable preview back to markdown. Every top-level child
// is a block, so anything the user adds (new paragraphs, headings, …) is kept.
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
  state.content = docFrontMatter() + serializeBody(preview);
  if (state.file) state.dirty.add(state.file);
  $("#editor").value = state.content; // keep the source view in sync (no input event)
  setSave("Editing…", "dirty");
  updateStats();
  clearTimeout(state.saveTimer);
  state.saveTimer = setTimeout(save, 900);
}

function setupPreviewEditing() {
  const preview = $("#preview");
  preview.addEventListener("input", () => {
    state.editing = true;
    syncFromPreview();
  });
  // Delete a whole block: Backspace/Delete on an emptied block (e.g. a heading
  // whose text you cleared) removes it and moves the caret to the neighbour.
  preview.addEventListener("keydown", (e) => {
    if (preview.contentEditable !== "true") return;
    if (e.key !== "Backspace" && e.key !== "Delete") return;
    const sel = window.getSelection();
    if (!sel || !sel.isCollapsed) return; // let the browser handle real selections
    const block = topBlock();
    if (!block || block.textContent.trim()) return; // only when the block is empty
    e.preventDefault();
    const neighbour =
      e.key === "Backspace" ? block.previousElementSibling : block.nextElementSibling;
    block.remove();
    state.editing = true;
    if (neighbour) {
      placeCaretEnd(neighbour);
    } else {
      preview.innerHTML = "";
      preview.focus();
    }
    syncFromPreview();
  });
  preview.addEventListener("focusout", (e) => {
    if (e.relatedTarget && preview.contains(e.relatedTarget)) return; // still inside
    state.editing = false;
    // Re-render once you leave the editor: applies markdown formatting and
    // normalizes the HTML. Focus already left, so there's no caret to disturb.
    clearTimeout(state.normalizeTimer);
    state.normalizeTimer = setTimeout(() => {
      if (!state.editing) renderPreview();
    }, 200);
  });
}

// ---- block formatting toolbar (add a title / change block type) ----
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

// ------------------------------------------------------------------- wire ---

function wire() {
  $("#btn-export").onclick = (e) => {
    e.stopPropagation();
    toggleExportMenu();
  };
  document.querySelectorAll(".export-opt").forEach((o) => {
    o.onclick = () => doExport(o.dataset.fmt);
  });
  $("#export-menu").addEventListener("click", (e) => e.stopPropagation());
  document.addEventListener("click", (e) => {
    if (!e.target.closest(".export-wrap")) hideExportMenu();
  });
  document.addEventListener("mousedown", (e) => {
    if (!e.target.closest("#ctx-menu")) hideCtxMenu();
  });
  document.addEventListener("scroll", hideCtxMenu, true);

  $("#btn-new").onclick = () => newUntitledFile(null); // date-named note; type the title
  $("#btn-new-group").onclick = () => showNew("group");
  makeDropTarget($("#files-head"), null); // drop here to move a note to the root
  $("#new-name").addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      commitNew();
    } else if (e.key === "Escape") {
      e.preventDefault();
      hideNew();
    }
  });

  $("#editor").addEventListener("input", onEdit);
  setupPreviewEditing();
  // Click the empty area below the content to keep writing (append a paragraph).
  $("#preview-scroll").addEventListener("mousedown", (e) => {
    const preview = $("#preview");
    if (preview.contentEditable !== "true") return;
    if (e.target !== preview && e.target !== $("#preview-scroll")) return; // clicked real content
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
  $("#preview-scroll").addEventListener("scroll", () => {
    if (state._raf) return;
    state._raf = requestAnimationFrame(() => {
      state._raf = 0;
      updateActiveHeading();
    });
  });

  document.querySelectorAll("#view-toggle button").forEach((b) => {
    b.onclick = () => setView(b.dataset.view);
  });
  document.querySelectorAll("#format-bar button").forEach((b) => {
    // preventDefault on mousedown keeps the caret in the contenteditable.
    b.addEventListener("mousedown", (e) => e.preventDefault());
    b.onclick = () => formatBlock(b.dataset.fmt);
  });
  $("#btn-toggle-tree").onclick = toggleSidebar;

  document.addEventListener("keydown", (e) => {
    if (!(e.metaKey || e.ctrlKey)) return;
    const k = e.key.toLowerCase();
    if (k === "s") {
      e.preventDefault();
      if (state.file) state.dirty.add(state.file);
      save();
    } else if (k === "b") {
      e.preventDefault();
      toggleSidebar();
    } else if (k === "e") {
      e.preventDefault();
      toggleExportMenu();
    } else if (k === "p") {
      e.preventDefault();
      doExport("pdf"); // print → export as PDF (Save dialog)
    } else if (k === "n") {
      e.preventDefault();
      newUntitledFile(null);
    } else if (k === "\\") {
      e.preventDefault();
      cycleView();
    } else if (k === "d") {
      e.preventDefault();
      if (state.file && state.file !== "papery.toml") deleteFileAction(state.file);
    }
  });
}

wire();
setView("reading"); // default: work directly in the preview (WYSIWYG)

(async () => {
  try {
    const p = await invoke("startup_project");
    if (p) await loadProject(p);
  } catch (e) {
    setSave("startup: " + e, "dirty");
  }
})();
