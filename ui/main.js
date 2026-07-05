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
  editCtx: null, // active in-preview block edit (source splice context)
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
// The list label: the note's title (front matter / heading / first line),
// falling back to the filename stem for empty or config files.
function labelOf(path) {
  const e = entryOf(path);
  if (e && e.title) return e.title;
  return path.split("/").pop().replace(/\.md$/i, "");
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
      tree.appendChild(h);
    }
    for (const path of g.files) {
      const row = document.createElement("div");
      row.className = "file-row" + (path === state.file ? " active" : "");
      row.innerHTML =
        `<span class="file-dot ${dotClass(path)}"></span>` +
        `<span class="file-name" title="${escapeHtml(path)}">${escapeHtml(labelOf(path))}</span>` +
        (state.dirty.has(path) ? `<span class="file-dirty"></span>` : "");
      row.onclick = () => openFile(path);
      // Double-click the name to rename inline (not the project config file).
      if (path !== "papery.toml") {
        const nameEl = row.querySelector(".file-name");
        nameEl.title = "Double-click to rename";
        nameEl.ondblclick = (e) => {
          e.stopPropagation();
          beginRename(path, nameEl);
        };
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
    $("#active-path").textContent = path;
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
  if (state.editCtx) return; // don't clobber an in-progress in-preview edit
  const isMd = state.file.endsWith(".md");
  const preview = $("#preview");
  if (!isMd) {
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
    preview.innerHTML = inner ? inner.innerHTML : html;
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

// ------------------------------------------------------------------ export ---

async function doExport(fmt) {
  hideExportMenu();
  if (!state.root || !state.file) return;
  const base = state.file.split("/").pop().replace(/\.[^.]+$/, "");
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

// A unique `untitled.md` path within `folder` (or the root when null).
function uniqueUntitled(folder) {
  const prefix = folder ? folder.replace(/\/+$/, "") + "/" : "";
  const existing = new Set(state.docs.map((d) => d.path));
  let n = 0;
  let name;
  do {
    name = `${prefix}untitled${n ? "-" + n : ""}.md`;
    n++;
  } while (existing.has(name));
  return name;
}

async function newUntitledFile(folder) {
  if (!state.root) return;
  const name = uniqueUntitled(folder);
  try {
    // Blank canvas — write an empty file so it's ready to type into.
    await invoke("write_file", { root: state.root, path: name, content: "" });
    state.docs = await invoke("list_docs", { root: state.root });
    renderTree();
    await openFile(name);
    setView(state.view === "reading" ? "split" : state.view);
    const ed = $("#editor");
    ed.focus();
    ed.setSelectionRange(0, 0);
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
    setView(state.view === "reading" ? "split" : state.view);
    $("#editor").focus();
  } catch (e) {
    setSave("Create failed: " + e, "dirty");
  }
}

// ------------------------------------------------------------ rename file ---

function beginRename(path, nameEl) {
  state.renaming = true;
  const base = path.split("/").pop();
  // Only the name is editable; the `.md` extension is fixed, so edit the stem.
  const stem = base.replace(/\.[A-Za-z][\w]*$/, "");
  const input = document.createElement("input");
  input.className = "rename-input";
  input.value = stem;
  input.title = "Rename (stays .md)";
  input.spellcheck = false;
  nameEl.replaceWith(input);
  input.focus();
  input.setSelectionRange(0, stem.length);

  let done = false;
  const finish = (commit) => {
    if (done) return;
    done = true;
    state.renaming = false;
    if (commit) commitRename(path, input.value);
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

async function commitRename(oldPath, value) {
  const name = value.trim();
  if (!name) return renderTree();
  const dir = oldPath.includes("/") ? oldPath.slice(0, oldPath.lastIndexOf("/") + 1) : "";
  const newPath = dir + forceMd(name); // extension is always .md
  state.renaming = false;
  if (newPath === oldPath) return renderTree();
  if (state.docs.some((d) => d.path === newPath)) {
    setSave("Name already exists", "dirty");
    return renderTree();
  }
  try {
    if (state.file === oldPath && state.dirty.has(oldPath)) await save();
    await invoke("rename_file", { root: state.root, from: oldPath, to: newPath });
    if (state.file === oldPath) {
      state.file = newPath;
      $("#active-path").textContent = newPath;
    }
    state.dirty.delete(oldPath);
    state.docs = await invoke("list_docs", { root: state.root });
    renderTree();
    setSave("Renamed", "saved");
  } catch (e) {
    setSave("Rename failed: " + e, "dirty");
    renderTree();
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
// The preview is the Rust-rendered HTML; in edit mode each top-level block is a
// contenteditable wrapper tagged with its source byte range (data-bs/data-be).
// Editing a block serializes just that block back to markdown and splices it
// into the source, so front matter and untouched blocks are preserved exactly.

const _enc = new TextEncoder();
const _dec = new TextDecoder();

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

function blockToMarkdown(wrapper) {
  const el = wrapper.firstElementChild || wrapper;
  return serializeBlock(el).replace(/\n{3,}/g, "\n\n").trim();
}

function setupPreviewEditing() {
  const preview = $("#preview");

  preview.addEventListener("focusin", (e) => {
    const block = e.target.closest(".pv-block");
    if (!block) return;
    const bs = +block.dataset.bs;
    const be = +block.dataset.be;
    const bytes = _enc.encode(state.content);
    state.editCtx = {
      block,
      bs,
      before: _dec.decode(bytes.slice(0, bs)),
      after: _dec.decode(bytes.slice(be)),
      origLen: be - bs,
    };
  });

  preview.addEventListener("input", (e) => {
    const ctx = state.editCtx;
    if (!ctx || !ctx.block.contains(e.target)) return;
    const md = blockToMarkdown(ctx.block);
    state.content = ctx.before + md + ctx.after;
    if (state.file) state.dirty.add(state.file);
    $("#editor").value = state.content; // keep source view in sync (no input event)
    ctx.block.dataset.be = ctx.bs + _enc.encode(md).length;
    setSave("Editing…", "dirty");
    updateStats();
    clearTimeout(state.saveTimer);
    state.saveTimer = setTimeout(save, 900);
  });

  preview.addEventListener("focusout", (e) => {
    const ctx = state.editCtx;
    if (!ctx) return;
    // Shift following blocks' offsets by this edit's byte delta so the next
    // block edited splices at the right place (no full re-render needed).
    const delta = _enc.encode(blockToMarkdown(ctx.block)).length - ctx.origLen;
    if (delta !== 0) {
      let sib = ctx.block.nextElementSibling;
      while (sib) {
        if (sib.classList && sib.classList.contains("pv-block")) {
          sib.dataset.bs = +sib.dataset.bs + delta;
          sib.dataset.be = +sib.dataset.be + delta;
        }
        sib = sib.nextElementSibling;
      }
    }
    state.editCtx = null;
    // Left the preview entirely → normalize with a fresh render.
    const to = e.relatedTarget;
    if (!to || !preview.contains(to)) {
      clearTimeout(state.normalizeTimer);
      state.normalizeTimer = setTimeout(() => {
        if (!state.editCtx) renderPreview();
      }, 300);
    }
  });
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

  $("#btn-new").onclick = () => showNew("file");
  $("#btn-new-group").onclick = () => showNew("group");
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
    } else if (k === "n") {
      e.preventDefault();
      newUntitledFile(null);
    } else if (k === "\\") {
      e.preventDefault();
      cycleView();
    }
  });
}

wire();
setView("split");

(async () => {
  try {
    const p = await invoke("startup_project");
    if (p) await loadProject(p);
  } catch (e) {
    setSave("startup: " + e, "dirty");
  }
})();
