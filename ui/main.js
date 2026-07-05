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

function groupDocs() {
  const all = state.docs.map((d) => d.path).slice();
  if (!all.includes("papery.toml")) all.push("papery.toml");
  const roots = all.filter((p) => !p.includes("/")).sort();
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
    .forEach((f) => groups.push({ folder: f, files: folders[f].sort() }));
  return groups;
}

function renderTree() {
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
        `<span class="file-name">${escapeHtml(path.split("/").pop())}</span>` +
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

function showNewGroup() {
  const box = $("#new-file");
  box.hidden = false;
  const input = $("#new-name");
  input.value = "";
  input.focus();
}
function hideNewGroup() {
  $("#new-file").hidden = true;
}
async function commitNewGroup() {
  const folder = $("#new-name").value.trim().replace(/^\/+|\/+$/g, "");
  hideNewGroup();
  if (!folder) return;
  // A group is a folder; create it by starting an untitled file inside it.
  await newUntitledFile(folder);
}

// ------------------------------------------------------------ rename file ---

function beginRename(path, nameEl) {
  const base = path.split("/").pop();
  const input = document.createElement("input");
  input.className = "rename-input";
  input.value = base;
  input.spellcheck = false;
  nameEl.replaceWith(input);
  input.focus();
  const dot = base.lastIndexOf(".");
  input.setSelectionRange(0, dot > 0 ? dot : base.length);

  let done = false;
  const finish = (commit) => {
    if (done) return;
    done = true;
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
  let base = value.trim();
  if (!base) return renderTree();
  if (!/\.[a-z0-9]+$/i.test(base)) {
    const ext = oldPath.includes(".") ? oldPath.slice(oldPath.lastIndexOf(".")) : ".md";
    base += ext;
  }
  const dir = oldPath.includes("/") ? oldPath.slice(0, oldPath.lastIndexOf("/") + 1) : "";
  const newPath = dir + base;
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
function cssEscape(s) {
  return window.CSS && CSS.escape ? CSS.escape(s) : s.replace(/"/g, '\\"');
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

  $("#btn-new").onclick = () => newUntitledFile(null);
  $("#btn-new-group").onclick = showNewGroup;
  $("#new-name").addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      commitNewGroup();
    } else if (e.key === "Escape") {
      e.preventDefault();
      hideNewGroup();
    }
  });

  $("#editor").addEventListener("input", onEdit);
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
