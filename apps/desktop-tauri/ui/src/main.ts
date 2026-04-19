import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { readFile } from "@tauri-apps/plugin-fs";

type PageSize = { width_pt: number; height_pt: number };
type RectPt = { x0: number; y0: number; x1: number; y1: number };
type OutlineNode = {
  title: string;
  page: number | null;
  children: OutlineNode[];
};
type OpenedDocument = {
  id: string;
  page_count: number;
  page_sizes: PageSize[];
  outline: OutlineNode[];
};
type Rendered = { width: number; height: number; data_url: string };
type SearchHit = { page: number; rects_pt: RectPt[]; snippet: string };
type TextRun = { text: string; rect_pt: RectPt };
type RecentFile = { path: string; opened_at_unix: number };

type FitMode = "custom" | "width" | "page" | "actual";

const app = document.getElementById("app")!;
app.innerHTML = `
<main class="root">
  <header class="topbar">
    <button id="open" title="Open (Ctrl+O)">Open</button>
    <button id="recentBtn" title="Recent files">Recent ▾</button>
    <div id="recentMenu" class="menu hidden"></div>
    <span class="sep"></span>
    <button id="prevPage" title="Previous page (PageUp)">◀</button>
    <input id="pageInput" type="number" min="1" value="1" style="width:4rem" />
    <span id="pageTotal">/ 0</span>
    <button id="nextPage" title="Next page (PageDown)">▶</button>
    <span class="sep"></span>
    <button id="zoomOut" title="Zoom out (Ctrl+-)">−</button>
    <select id="fitSelect" title="Fit mode">
      <option value="custom">Custom</option>
      <option value="width">Fit Width</option>
      <option value="page">Fit Page</option>
      <option value="actual">100% (Actual)</option>
    </select>
    <span id="zoomLabel">100%</span>
    <button id="zoomIn" title="Zoom in (Ctrl++)">+</button>
    <span class="sep"></span>
    <button id="searchBtn" title="Search (Ctrl+F)">Search</button>
    <span id="status" class="status"></span>
  </header>
  <div class="body">
    <aside id="sidebar" class="sidebar">
      <div class="tabs">
        <button class="tab active" data-tab="thumbs">Thumbnails</button>
        <button class="tab" data-tab="outline">Outline</button>
      </div>
      <div id="thumbsPanel" class="panel"></div>
      <div id="outlinePanel" class="panel hidden"></div>
    </aside>
    <div id="viewer" class="viewer">
      <div id="pageWrap" class="pageWrap hidden">
        <img id="page" class="page" />
        <div id="selectLayer" class="selectLayer"></div>
        <div id="highlightLayer" class="highlightLayer"></div>
      </div>
    </div>
    <div id="searchPanel" class="searchPanel hidden">
      <input id="searchInput" placeholder="Find in document…" />
      <label><input id="searchCase" type="checkbox" /> Case</label>
      <button id="searchPrev" title="Previous match (Shift+F3)">◀</button>
      <button id="searchNext" title="Next match (F3)">▶</button>
      <span id="searchCount"></span>
      <button id="searchClose" title="Close (Esc)">✕</button>
    </div>
  </div>
  <div id="dropOverlay" class="dropOverlay hidden">Drop PDF here</div>
</main>
`;

const style = document.createElement("style");
style.textContent = `
  html, body { margin:0; padding:0; height:100%; font-family: system-ui; background:#1e1e1e; color:#ddd; }
  #app, .root { height: 100%; }
  .root { display:flex; flex-direction:column; }
  .topbar {
    padding: 0.4rem 0.6rem; border-bottom: 1px solid #333;
    display: flex; gap: 0.4rem; align-items: center; background:#252526; position:relative;
  }
  .topbar button, .topbar select, .topbar input {
    background:#3a3a3a; color:#ddd; border:1px solid #4a4a4a; border-radius:3px;
    padding:0.2rem 0.5rem; font: inherit;
  }
  .topbar button:hover { background:#4a4a4a; }
  .sep { display:inline-block; width:1px; height:20px; background:#444; margin: 0 0.2rem; }
  .status { margin-left:auto; color:#999; font-variant-numeric: tabular-nums; font-size: 0.85rem; }
  .body { flex:1; display:flex; min-height: 0; position:relative; }
  .sidebar { width: 200px; background:#252526; border-right:1px solid #333; display:flex; flex-direction:column; min-height:0; }
  .tabs { display:flex; border-bottom:1px solid #333; }
  .tab { flex:1; padding:0.4rem; background:#2a2a2b; color:#bbb; border:none; cursor:pointer; }
  .tab.active { background:#1e1e1e; color:#fff; border-bottom: 2px solid #4a8cff; }
  .panel { flex:1; overflow:auto; padding:0.4rem; }
  .panel.hidden { display:none; }
  .thumb { display:block; margin: 0 auto 0.4rem; cursor:pointer; background:white;
           border:2px solid transparent; max-width: 100%; box-shadow: 0 1px 3px rgba(0,0,0,0.4); }
  .thumb.active { border-color:#4a8cff; }
  .thumbLabel { text-align:center; font-size:0.7rem; color:#888; margin-bottom:0.6rem; }
  .outlineItem { padding: 0.15rem 0.3rem; cursor:pointer; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; font-size:0.85rem; }
  .outlineItem:hover { background:#333; }
  .outlineChildren { margin-left: 0.8rem; border-left:1px solid #333; padding-left:0.3rem; }
  .viewer { flex:1; overflow:auto; background:#2b2b2b; display:flex; justify-content:center;
            align-items:flex-start; padding:1rem; }
  .pageWrap { position:relative; display:inline-block; }
  .pageWrap.hidden { display:none; }
  .page { background:white; box-shadow: 0 2px 8px rgba(0,0,0,0.4); display:block; }
  .selectLayer { position:absolute; inset:0; cursor:text; color:transparent; user-select:text;
                 pointer-events:auto; line-height: 1; }
  .selectLayer span { position:absolute; white-space:pre; color:transparent; user-select:text;
                      -webkit-user-select:text; transform-origin: 0 0; }
  .selectLayer span::selection { background: rgba(74,140,255,0.4); color:transparent; }
  .highlightLayer { position:absolute; inset:0; pointer-events:none; }
  .highlightLayer .hit { position:absolute; background: rgba(255,220,0,0.45); }
  .highlightLayer .hit.current { background: rgba(255,140,0,0.6); outline:1px solid orange; }
  .searchPanel { position:absolute; top: 0.5rem; right: 1rem; background:#2d2d30;
                 border:1px solid #444; border-radius:4px; padding: 0.3rem;
                 display:flex; gap:0.3rem; align-items:center; z-index: 20; }
  .searchPanel.hidden { display:none; }
  .searchPanel input[type=text], .searchPanel #searchInput {
    background:#3a3a3a; color:#ddd; border:1px solid #555; padding:0.2rem 0.4rem; width:16rem;
  }
  .menu { position:absolute; top:2.2rem; left:0.6rem; background:#2d2d30; border:1px solid #444;
          border-radius:4px; padding:0.3rem 0; min-width: 22rem; z-index:30; max-height:60vh; overflow:auto; }
  .menu.hidden { display:none; }
  .menu .item { padding: 0.35rem 0.7rem; cursor:pointer; font-size:0.85rem; white-space:nowrap;
                overflow:hidden; text-overflow:ellipsis; }
  .menu .item:hover { background:#3a3a3a; }
  .menu .empty { padding:0.5rem 0.7rem; color:#888; }
  .dropOverlay { position:fixed; inset:0; background: rgba(74,140,255,0.25);
                 border: 4px dashed #4a8cff; display:flex; align-items:center;
                 justify-content:center; font-size: 2rem; color:white; pointer-events:none; z-index:1000; }
  .dropOverlay.hidden { display:none; }
`;
document.head.appendChild(style);

const statusEl = document.getElementById("status") as HTMLSpanElement;
const viewer = document.getElementById("viewer") as HTMLDivElement;
const pageWrap = document.getElementById("pageWrap") as HTMLDivElement;
const pageImg = document.getElementById("page") as HTMLImageElement;
const selectLayer = document.getElementById("selectLayer") as HTMLDivElement;
const highlightLayer = document.getElementById("highlightLayer") as HTMLDivElement;
const zoomLabel = document.getElementById("zoomLabel") as HTMLSpanElement;
const fitSelect = document.getElementById("fitSelect") as HTMLSelectElement;
const pageInput = document.getElementById("pageInput") as HTMLInputElement;
const pageTotal = document.getElementById("pageTotal") as HTMLSpanElement;
const searchPanel = document.getElementById("searchPanel") as HTMLDivElement;
const searchInput = document.getElementById("searchInput") as HTMLInputElement;
const searchCase = document.getElementById("searchCase") as HTMLInputElement;
const searchCount = document.getElementById("searchCount") as HTMLSpanElement;
const thumbsPanel = document.getElementById("thumbsPanel") as HTMLDivElement;
const outlinePanel = document.getElementById("outlinePanel") as HTMLDivElement;
const recentMenu = document.getElementById("recentMenu") as HTMLDivElement;
const dropOverlay = document.getElementById("dropOverlay") as HTMLDivElement;

let currentDoc: string | null = null;
let pageCount = 0;
let pageSizes: PageSize[] = [];
let outline: OutlineNode[] = [];
let currentPage = 0;
let zoom = 1.0;
let fitMode: FitMode = "width";
let rendering = false;
let pendingRender = false;
let hits: SearchHit[] = [];
let currentHit = -1;
let textRunsCache: Map<number, TextRun[]> = new Map();
const thumbEls: HTMLImageElement[] = [];

function setStatus(s: string) {
  statusEl.textContent = s;
}

function updateChrome() {
  if (!currentDoc) {
    pageInput.value = "1";
    pageTotal.textContent = "/ 0";
    zoomLabel.textContent = `${Math.round(zoom * 100)}%`;
    return;
  }
  pageInput.value = String(currentPage + 1);
  pageInput.max = String(pageCount);
  pageTotal.textContent = `/ ${pageCount}`;
  zoomLabel.textContent = `${Math.round(zoom * 100)}%`;
  thumbEls.forEach((img, i) => img.classList.toggle("active", i === currentPage));
}

function computeZoom(): number {
  if (!currentDoc || !pageSizes[currentPage]) return zoom;
  const sz = pageSizes[currentPage];
  const vw = viewer.clientWidth - 32;
  const vh = viewer.clientHeight - 32;
  switch (fitMode) {
    case "width":
      return clampZoom(vw / sz.width_pt);
    case "page":
      return clampZoom(Math.min(vw / sz.width_pt, vh / sz.height_pt));
    case "actual":
      return 1.0;
    case "custom":
    default:
      return zoom;
  }
}

function clampZoom(z: number) {
  return Math.max(0.1, Math.min(10, z));
}

async function renderCurrent() {
  if (!currentDoc) return;
  if (rendering) {
    pendingRender = true;
    return;
  }
  rendering = true;
  try {
    zoom = computeZoom();
    const target = currentPage;
    const res = await invoke<Rendered>("render_page", {
      document: currentDoc,
      page: target,
      zoom,
    });
    pageImg.src = res.data_url;
    pageImg.width = res.width;
    pageImg.height = res.height;
    pageWrap.style.width = `${res.width}px`;
    pageWrap.style.height = `${res.height}px`;
    pageWrap.classList.remove("hidden");
    await ensureTextLayer(target);
    renderHighlights();
    updateChrome();
  } catch (e) {
    setStatus(`Render error: ${e}`);
  } finally {
    rendering = false;
    if (pendingRender) {
      pendingRender = false;
      renderCurrent();
    }
  }
}

async function ensureTextLayer(pageIdx: number) {
  if (!currentDoc) return;
  let runs = textRunsCache.get(pageIdx);
  if (!runs) {
    try {
      runs = await invoke<TextRun[]>("page_text_layout_cmd", {
        document: currentDoc,
        page: pageIdx,
      });
      textRunsCache.set(pageIdx, runs);
    } catch {
      runs = [];
    }
  }
  const sz = pageSizes[pageIdx];
  if (!sz) return;
  const scaleX = pageImg.width / sz.width_pt;
  const scaleY = pageImg.height / sz.height_pt;
  selectLayer.innerHTML = "";
  for (const run of runs) {
    const r = run.rect_pt;
    const left = r.x0 * scaleX;
    const top = r.y0 * scaleY;
    const width = (r.x1 - r.x0) * scaleX;
    const height = (r.y1 - r.y0) * scaleY;
    if (width <= 0 || height <= 0) continue;
    const span = document.createElement("span");
    span.textContent = run.text;
    span.style.left = `${left}px`;
    span.style.top = `${top}px`;
    span.style.fontSize = `${height}px`;
    span.style.height = `${height}px`;
    span.style.width = `${width}px`;
    selectLayer.appendChild(span);
  }
}

function renderHighlights() {
  highlightLayer.innerHTML = "";
  if (!currentDoc) return;
  const sz = pageSizes[currentPage];
  if (!sz) return;
  const scaleX = pageImg.width / sz.width_pt;
  const scaleY = pageImg.height / sz.height_pt;
  hits.forEach((hit, idx) => {
    if (hit.page !== currentPage) return;
    for (const r of hit.rects_pt) {
      const el = document.createElement("div");
      el.className = "hit" + (idx === currentHit ? " current" : "");
      el.style.left = `${r.x0 * scaleX}px`;
      el.style.top = `${r.y0 * scaleY}px`;
      el.style.width = `${(r.x1 - r.x0) * scaleX}px`;
      el.style.height = `${(r.y1 - r.y0) * scaleY}px`;
      highlightLayer.appendChild(el);
    }
  });
}

function goToPage(idx: number) {
  const clamped = Math.max(0, Math.min(pageCount - 1, idx));
  if (clamped !== currentPage) {
    currentPage = clamped;
    renderCurrent();
  }
}

async function openFromBytes(bytes: Uint8Array, label: string) {
  setStatus(`Opening ${label}…`);
  try {
    const doc = await invoke<OpenedDocument>("open_pdf", {
      bytes: Array.from(bytes),
    });
    currentDoc = doc.id;
    pageCount = doc.page_count;
    pageSizes = doc.page_sizes;
    outline = doc.outline;
    currentPage = 0;
    hits = [];
    currentHit = -1;
    textRunsCache.clear();
    searchCount.textContent = "";
    renderOutline();
    await renderCurrent();
    renderThumbsShell();
    setStatus(`${label} — ${pageCount} pages`);
  } catch (e) {
    setStatus(`Error: ${e}`);
  }
}

async function openByPath(path: string) {
  const bytes = await readFile(path);
  await openFromBytes(bytes, path);
  try {
    await invoke("push_recent", { path });
  } catch {}
}

document.getElementById("open")!.addEventListener("click", async () => {
  const path = await open({
    multiple: false,
    filters: [{ name: "PDF", extensions: ["pdf"] }],
  });
  if (!path || typeof path !== "string") return;
  await openByPath(path);
});

document.getElementById("recentBtn")!.addEventListener("click", async (ev) => {
  ev.stopPropagation();
  if (!recentMenu.classList.contains("hidden")) {
    recentMenu.classList.add("hidden");
    return;
  }
  const list = await invoke<RecentFile[]>("recent_files").catch(() => []);
  recentMenu.innerHTML = "";
  if (list.length === 0) {
    const div = document.createElement("div");
    div.className = "empty";
    div.textContent = "No recent files";
    recentMenu.appendChild(div);
  } else {
    for (const f of list) {
      const div = document.createElement("div");
      div.className = "item";
      div.title = f.path;
      div.textContent = f.path;
      div.addEventListener("click", async () => {
        recentMenu.classList.add("hidden");
        await openByPath(f.path);
      });
      recentMenu.appendChild(div);
    }
  }
  recentMenu.classList.remove("hidden");
});
document.addEventListener("click", () => recentMenu.classList.add("hidden"));

document.getElementById("prevPage")!.addEventListener("click", () => goToPage(currentPage - 1));
document.getElementById("nextPage")!.addEventListener("click", () => goToPage(currentPage + 1));
pageInput.addEventListener("change", () => {
  const v = parseInt(pageInput.value, 10);
  if (!isNaN(v)) goToPage(v - 1);
});

document.getElementById("zoomIn")!.addEventListener("click", () => {
  fitMode = "custom";
  fitSelect.value = "custom";
  zoom = clampZoom(zoom + 0.1);
  renderCurrent();
});
document.getElementById("zoomOut")!.addEventListener("click", () => {
  fitMode = "custom";
  fitSelect.value = "custom";
  zoom = clampZoom(zoom - 0.1);
  renderCurrent();
});
fitSelect.addEventListener("change", () => {
  fitMode = fitSelect.value as FitMode;
  renderCurrent();
});
fitSelect.value = fitMode;

viewer.addEventListener(
  "wheel",
  (e: WheelEvent) => {
    if (!currentDoc) return;
    if (e.ctrlKey) {
      e.preventDefault();
      fitMode = "custom";
      fitSelect.value = "custom";
      const delta = e.deltaY > 0 ? -0.1 : 0.1;
      zoom = clampZoom(zoom + delta);
      renderCurrent();
      return;
    }
    const atTop = viewer.scrollTop <= 0;
    const atBottom = viewer.scrollTop + viewer.clientHeight >= viewer.scrollHeight - 1;
    if (e.deltaY < 0 && atTop && currentPage > 0) {
      e.preventDefault();
      currentPage -= 1;
      renderCurrent().then(() => (viewer.scrollTop = viewer.scrollHeight));
    } else if (e.deltaY > 0 && atBottom && currentPage < pageCount - 1) {
      e.preventDefault();
      currentPage += 1;
      renderCurrent().then(() => (viewer.scrollTop = 0));
    }
  },
  { passive: false },
);

function renderOutline() {
  outlinePanel.innerHTML = "";
  if (outline.length === 0) {
    outlinePanel.innerHTML = '<div style="color:#777;padding:0.4rem;font-size:0.85rem;">No outline</div>';
    return;
  }
  const tree = buildOutlineTree(outline);
  outlinePanel.appendChild(tree);
}

function buildOutlineTree(nodes: OutlineNode[]): HTMLElement {
  const wrap = document.createElement("div");
  for (const n of nodes) {
    const item = document.createElement("div");
    item.className = "outlineItem";
    item.textContent = n.title || "(untitled)";
    item.title = n.title;
    if (n.page !== null) {
      const page = n.page;
      item.addEventListener("click", () => goToPage(page));
    }
    wrap.appendChild(item);
    if (n.children.length > 0) {
      const sub = buildOutlineTree(n.children);
      sub.className = "outlineChildren";
      wrap.appendChild(sub);
    }
  }
  return wrap;
}

function renderThumbsShell() {
  thumbsPanel.innerHTML = "";
  thumbEls.length = 0;
  for (let i = 0; i < pageCount; i++) {
    const img = document.createElement("img");
    img.className = "thumb";
    img.loading = "lazy";
    img.dataset.page = String(i);
    img.addEventListener("click", () => goToPage(i));
    const label = document.createElement("div");
    label.className = "thumbLabel";
    label.textContent = `${i + 1}`;
    thumbsPanel.appendChild(img);
    thumbsPanel.appendChild(label);
    thumbEls.push(img);
  }
  requestIdleCallback
    ? requestIdleCallback(() => loadThumbs())
    : setTimeout(loadThumbs, 50);
}

async function loadThumbs() {
  if (!currentDoc) return;
  const doc = currentDoc;
  for (let i = 0; i < pageCount; i++) {
    if (doc !== currentDoc) return;
    const sz = pageSizes[i];
    if (!sz) continue;
    try {
      const res = await invoke<Rendered>("render_thumbnail", {
        document: doc,
        page: i,
        maxWidth: 170,
        pageWidthPt: sz.width_pt,
      });
      thumbEls[i].src = res.data_url;
    } catch {}
  }
}

document.querySelectorAll(".tab").forEach((el) => {
  el.addEventListener("click", () => {
    document.querySelectorAll(".tab").forEach((t) => t.classList.remove("active"));
    el.classList.add("active");
    const tab = (el as HTMLElement).dataset.tab;
    thumbsPanel.classList.toggle("hidden", tab !== "thumbs");
    outlinePanel.classList.toggle("hidden", tab !== "outline");
  });
});

function openSearch() {
  if (!currentDoc) return;
  searchPanel.classList.remove("hidden");
  searchInput.focus();
  searchInput.select();
}
function closeSearch() {
  searchPanel.classList.add("hidden");
  hits = [];
  currentHit = -1;
  searchCount.textContent = "";
  renderHighlights();
}
document.getElementById("searchBtn")!.addEventListener("click", openSearch);
document.getElementById("searchClose")!.addEventListener("click", closeSearch);

async function runSearch() {
  if (!currentDoc) return;
  const term = searchInput.value.trim();
  if (!term) {
    hits = [];
    currentHit = -1;
    searchCount.textContent = "";
    renderHighlights();
    return;
  }
  try {
    hits = await invoke<SearchHit[]>("search_document", {
      document: currentDoc,
      term,
      caseSensitive: searchCase.checked,
    });
    currentHit = hits.length > 0 ? 0 : -1;
    searchCount.textContent = hits.length === 0 ? "No matches" : `${hits.length} matches`;
    if (currentHit >= 0) jumpToHit(0);
  } catch (e) {
    searchCount.textContent = `Error: ${e}`;
  }
}

function jumpToHit(idx: number) {
  if (hits.length === 0) return;
  currentHit = (idx + hits.length) % hits.length;
  searchCount.textContent = `${currentHit + 1} / ${hits.length}`;
  const target = hits[currentHit].page;
  if (target !== currentPage) {
    currentPage = target;
    renderCurrent();
  } else {
    renderHighlights();
  }
}

let searchDebounce: number | null = null;
searchInput.addEventListener("input", () => {
  if (searchDebounce) clearTimeout(searchDebounce);
  searchDebounce = window.setTimeout(runSearch, 250);
});
searchInput.addEventListener("keydown", (e) => {
  if (e.key === "Enter") {
    e.preventDefault();
    if (hits.length === 0) runSearch();
    else jumpToHit(currentHit + (e.shiftKey ? -1 : 1));
  } else if (e.key === "Escape") {
    closeSearch();
  }
});
searchCase.addEventListener("change", runSearch);
document.getElementById("searchPrev")!.addEventListener("click", () => jumpToHit(currentHit - 1));
document.getElementById("searchNext")!.addEventListener("click", () => jumpToHit(currentHit + 1));

window.addEventListener("keydown", (e) => {
  const inField = document.activeElement && /INPUT|TEXTAREA/.test(document.activeElement.tagName);
  if (e.ctrlKey && (e.key === "o" || e.key === "O")) {
    e.preventDefault();
    (document.getElementById("open") as HTMLButtonElement).click();
    return;
  }
  if (e.ctrlKey && (e.key === "f" || e.key === "F")) {
    e.preventDefault();
    openSearch();
    return;
  }
  if (e.key === "F3" || (e.ctrlKey && (e.key === "g" || e.key === "G"))) {
    e.preventDefault();
    if (hits.length > 0) jumpToHit(currentHit + (e.shiftKey ? -1 : 1));
    return;
  }
  if (e.ctrlKey && (e.key === "+" || e.key === "=")) {
    e.preventDefault();
    (document.getElementById("zoomIn") as HTMLButtonElement).click();
    return;
  }
  if (e.ctrlKey && e.key === "-") {
    e.preventDefault();
    (document.getElementById("zoomOut") as HTMLButtonElement).click();
    return;
  }
  if (e.ctrlKey && e.key === "0") {
    e.preventDefault();
    fitMode = "actual";
    fitSelect.value = "actual";
    renderCurrent();
    return;
  }
  if (e.ctrlKey && (e.key === "1")) {
    e.preventDefault();
    fitMode = "page";
    fitSelect.value = "page";
    renderCurrent();
    return;
  }
  if (e.ctrlKey && (e.key === "2")) {
    e.preventDefault();
    fitMode = "width";
    fitSelect.value = "width";
    renderCurrent();
    return;
  }
  if (inField) return;
  if (!currentDoc) return;
  if (e.key === "ArrowDown" || e.key === "PageDown") {
    e.preventDefault();
    goToPage(currentPage + 1);
  } else if (e.key === "ArrowUp" || e.key === "PageUp") {
    e.preventDefault();
    goToPage(currentPage - 1);
  } else if (e.key === "Home") {
    e.preventDefault();
    goToPage(0);
  } else if (e.key === "End") {
    e.preventDefault();
    goToPage(pageCount - 1);
  } else if (e.key === "Escape") {
    if (!searchPanel.classList.contains("hidden")) closeSearch();
  }
});

window.addEventListener("resize", () => {
  if (fitMode === "width" || fitMode === "page") renderCurrent();
});

getCurrentWebview()
  .onDragDropEvent(async (event) => {
    const p = event.payload;
    if (p.type === "over" || p.type === "enter") {
      dropOverlay.classList.remove("hidden");
    } else if (p.type === "leave") {
      dropOverlay.classList.add("hidden");
    } else if (p.type === "drop") {
      dropOverlay.classList.add("hidden");
      const paths = (p as { paths: string[] }).paths;
      const pdf = paths.find((f) => /\.pdf$/i.test(f));
      if (pdf) await openByPath(pdf);
    }
  })
  .catch(() => {});

updateChrome();
