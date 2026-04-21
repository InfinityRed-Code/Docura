import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open, save, confirm as confirmDialog } from "@tauri-apps/plugin-dialog";
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
type LinkTarget = { kind: "Page"; value: number } | { kind: "Uri"; value: string };
type PageLink = { rect_pt: RectPt; target: LinkTarget };
type AttachmentInfo = { index: number; name: string; size: number };
type DocMetadata = {
  title: string | null;
  author: string | null;
  subject: string | null;
  keywords: string | null;
  creator: string | null;
  producer: string | null;
  creation_date: string | null;
  modification_date: string | null;
};
type DocPermissions = {
  security_handler: string;
  can_print_high_quality: boolean;
  can_print_low_quality: boolean;
  can_modify_content: boolean;
  can_extract_text_and_graphics: boolean;
  can_annotate: boolean;
  can_fill_forms: boolean;
  can_assemble: boolean;
};

type FitMode = "custom" | "width" | "page" | "actual";
type ViewMode = "single" | "continuous" | "double";

const app = document.getElementById("app")!;
app.innerHTML = `
<main class="root">
  <header class="ribbon">
    <div class="ribbonTabs">
      <button class="ribbonTab active" data-ribbon-tab="reader">Reader</button>
      <button class="ribbonTab" data-ribbon-tab="edit">Edit</button>
      <button class="ribbonTab" data-ribbon-tab="organize">Organize</button>
    </div>
    <div class="ribbonBody" data-active="reader">
      <button data-ribbon="both" id="sidebarBtn" title="Toggle sidebar (F4)">☰</button>
      <button data-ribbon="both" id="open" title="Open (Ctrl+O)">Open</button>
      <button data-ribbon="both" id="recentBtn" title="Recent files">Recent ▾</button>
      <div id="recentMenu" class="menu hidden"></div>
      <span data-ribbon="both" class="sep"></span>
      <button data-ribbon="both" id="backBtn" title="Back (Alt+Left)" disabled>←</button>
      <button data-ribbon="both" id="forwardBtn" title="Forward (Alt+Right)" disabled>→</button>
      <span data-ribbon="both" class="sep"></span>
      <button data-ribbon="both" id="prevPage" title="Previous page (PageUp)">◀</button>
      <input data-ribbon="both" id="pageInput" type="number" min="1" value="1" style="width:4rem" />
      <span data-ribbon="both" id="pageTotal">/ 0</span>
      <button data-ribbon="both" id="nextPage" title="Next page (PageDown)">▶</button>
      <button data-ribbon="both" id="gotoBtn" title="Go to page (Ctrl+G)">⇥</button>
      <span data-ribbon="reader" class="sep"></span>
      <button data-ribbon="reader" id="zoomOut" title="Zoom out (Ctrl+-)">−</button>
      <select data-ribbon="reader" id="fitSelect" title="Fit mode">
        <option value="custom">Custom</option>
        <option value="width">Fit Width</option>
        <option value="page">Fit Page</option>
        <option value="actual">100% (Actual)</option>
      </select>
      <select data-ribbon="reader" id="zoomSelect" title="Zoom">
        <option value="custom" id="zoomCustomOpt">100%</option>
        <option value="0.1">10%</option>
        <option value="0.5">50%</option>
        <option value="0.8">80%</option>
        <option value="1">100%</option>
        <option value="1.2">120%</option>
        <option value="1.5">150%</option>
        <option value="2">200%</option>
      </select>
      <button data-ribbon="reader" id="zoomIn" title="Zoom in (Ctrl++)">+</button>
      <span data-ribbon="reader" class="sep"></span>
      <select data-ribbon="reader" id="viewModeSelect" title="View mode">
        <option value="single">Single Page</option>
        <option value="continuous">Continuous</option>
        <option value="double">Two Pages</option>
      </select>
      <button data-ribbon="reader" id="rotLeft" title="Rotate left (Ctrl+Shift+L for doc)">↶</button>
      <button data-ribbon="reader" id="rotRight" title="Rotate right (Ctrl+Shift+R for doc)">↷</button>
      <button data-ribbon="reader" id="darkToggle" title="Invert colors">☾</button>
      <button data-ribbon="reader" id="fullscreenBtn" title="Fullscreen (F11)">⛶</button>
      <button data-ribbon="reader" id="presentBtn" title="Presentation (F5)">▶▶</button>
      <span data-ribbon="reader" class="sep"></span>
      <button data-ribbon="reader" id="searchBtn" title="Search (Ctrl+F)">Search</button>
      <button data-ribbon="both" id="saveAsBtn" title="Save As (Ctrl+Shift+S)">💾 Save As</button>
      <button data-ribbon="reader" id="printBtn" title="Print (Ctrl+P)">🖨</button>
      <button data-ribbon="reader" id="propsBtn" title="Document properties">ⓘ</button>
      <span data-ribbon="edit" class="sep"></span>
      <span data-ribbon="edit" class="ribbonLabel">Mode:</span>
      <div data-ribbon="edit" class="modeGroup" role="radiogroup">
        <button class="modeBtn active" data-mode="select" title="Select (no page edits)">👆 Select</button>
        <button class="modeBtn" data-mode="delete" title="Select pages with click / Ctrl+click / Shift+click, then press the trash button">🗑 Delete mode</button>
      </div>
      <button data-ribbon="edit" id="deleteSelectionBtn" class="deleteAction" disabled title="Delete selected pages">🗑 Delete selected</button>
      <button data-ribbon="edit" id="extractSelectionBtn" disabled title="Save selected pages as a new PDF">⬇ Extract selected</button>
      <span data-ribbon="edit" class="sep"></span>
      <div data-ribbon="edit" class="menuHost">
        <button id="rotateMenuBtn" class="menuTrigger" aria-haspopup="true" aria-expanded="false" title="Rotate pages">⟳ Rotate ▾</button>
        <div id="rotateMenu" class="popover hidden" role="menu">
          <button id="rotateLeftBtn" role="menuitem" title="Rotate 90° counter-clockwise">↺ 90°</button>
          <button id="rotateRightBtn" role="menuitem" title="Rotate 90° clockwise">↻ 90°</button>
          <button id="rotate180Btn" role="menuitem" title="Rotate 180°">↻ 180°</button>
        </div>
      </div>
      <span data-ribbon="edit" id="modeHint" class="modeHint"></span>
      <span data-ribbon="organize" id="organizeHint" class="modeHint">Drag thumbnails to reorder. Click / Ctrl+click / Shift+click to select multiple.</span>
      <span data-ribbon="both" id="status" class="status"></span>
    </div>
  </header>
  <div class="body">
    <aside id="sidebar" class="sidebar hidden">
      <div class="tabs">
        <button class="tab active" data-tab="thumbs">Thumbs</button>
        <button class="tab" data-tab="outline">Outline</button>
        <button class="tab" data-tab="attachments">Files</button>
      </div>
      <div id="thumbsPanel" class="panel"></div>
      <div id="outlinePanel" class="panel hidden"></div>
      <div id="attachmentsPanel" class="panel hidden"></div>
    </aside>
    <div id="viewer" class="viewer">
      <div id="pageList" class="pageList"></div>
    </div>
    <div id="searchPanel" class="searchPanel hidden">
      <input id="searchInput" placeholder="Find in document…" />
      <label><input id="searchCase" type="checkbox" /> Case</label>
      <button id="searchPrev" title="Previous match (Shift+F3)">◀</button>
      <button id="searchNext" title="Next match (F3)">▶</button>
      <span id="searchCount"></span>
      <button id="searchClose" title="Close (Esc)">✕</button>
    </div>
    <div id="gotoPanel" class="gotoPanel hidden">
      <label>Go to page</label>
      <input id="gotoInput" type="number" min="1" />
      <button id="gotoGo">Go</button>
      <button id="gotoClose">✕</button>
    </div>
  </div>
  <div id="dropOverlay" class="dropOverlay hidden">Drop PDF here</div>
  <div id="propsOverlay" class="propsOverlay hidden">
    <div class="propsModal">
      <div class="propsHeader">
        <div class="propsTabs">
          <button class="propsTab active" data-ptab="info">Info</button>
          <button class="propsTab" data-ptab="perms">Permissions</button>
        </div>
        <button id="propsClose" class="propsClose">✕</button>
      </div>
      <div id="propsInfo" class="propsPanel"></div>
      <div id="propsPerms" class="propsPanel hidden"></div>
    </div>
  </div>
</main>
`;

const style = document.createElement("style");
style.textContent = `
  html, body { margin:0; padding:0; height:100%; font-family: system-ui; background:#1e1e1e; color:#ddd; }
  #app, .root { height: 100%; }
  .root { display:flex; flex-direction:column; }
  .ribbon { border-bottom: 1px solid #333; background:#252526; position:relative; }
  .ribbonTabs { display:flex; gap:0; border-bottom:1px solid #2f2f2f; padding: 0 0.4rem; }
  .ribbonTab { background:transparent; color:#bbb; border:none; padding:0.35rem 0.9rem;
               cursor:pointer; font:inherit; font-size:0.88rem; border-radius:3px 3px 0 0; }
  .ribbonTab:hover { background:#2f2f31; color:#ddd; }
  .ribbonTab.active { background:#1e1e1e; color:#fff; border-bottom:2px solid #4a8cff; }
  .ribbonBody { padding: 0.4rem 0.6rem; display:flex; gap: 0.4rem; align-items:center;
                flex-wrap: wrap; }
  .ribbonBody button, .ribbonBody select, .ribbonBody input {
    background:#3a3a3a; color:#ddd; border:1px solid #4a4a4a; border-radius:3px;
    padding:0.2rem 0.5rem; font: inherit;
  }
  .ribbonBody button:hover:not(:disabled) { background:#4a4a4a; }
  .ribbonBody button:disabled { opacity:0.4; cursor:default; }
  .ribbonBody[data-active="reader"] > [data-ribbon="edit"],
  .ribbonBody[data-active="reader"] > [data-ribbon="organize"] { display:none; }
  .ribbonBody[data-active="edit"] > [data-ribbon="reader"],
  .ribbonBody[data-active="edit"] > [data-ribbon="organize"] { display:none; }
  .ribbonBody[data-active="organize"] > [data-ribbon="reader"],
  .ribbonBody[data-active="organize"] > [data-ribbon="edit"] { display:none; }
  .ribbonLabel { color:#888; font-size:0.85rem; margin-left:0.2rem; }
  .modeGroup { display:inline-flex; gap: 0; border:1px solid #4a4a4a; border-radius:4px;
               overflow:hidden; }
  .modeGroup .modeBtn { background:#3a3a3a; color:#ddd; border:none; border-right:1px solid #4a4a4a;
                        padding:0.25rem 0.6rem; cursor:pointer; font:inherit; font-size:0.88rem; }
  .modeGroup .modeBtn:last-child { border-right:none; }
  .modeGroup .modeBtn:hover { background:#4a4a4a; }
  .modeGroup .modeBtn.active { background:#4a8cff; color:#fff; }
  .modeGroup .modeBtn.active.danger { background:#c0392b; }
  .modeHint { color:#999; font-size:0.82rem; margin-left:0.3rem; }
  body.edit-delete .modeHint { color:#c0392b; font-weight:500; }
  .menuHost { position:relative; display:inline-block; }
  .popover { position:absolute; top:calc(100% + 4px); left:0; background:#2a2a2a;
             border:1px solid #444; border-radius:4px; box-shadow:0 4px 12px rgba(0,0,0,0.5);
             padding:0.25rem; display:flex; flex-direction:column; min-width:120px; z-index:100; }
  .popover.hidden { display:none; }
  .popover button { background:transparent; color:#ddd; border:none; padding:0.4rem 0.6rem;
                    text-align:left; border-radius:3px; cursor:pointer; font-size:0.9rem; }
  .popover button:hover { background:#3a3a3a; }
  .menuTrigger[aria-expanded="true"] { background:#3a3a3a; }
  body.edit-delete .thumb { outline:1px dashed transparent; cursor: pointer; }
  body.edit-delete .thumb:hover { outline:2px solid #c0392b; }
  .thumb.selected { outline:2px solid #4a8cff; box-shadow: 0 0 0 3px rgba(74,140,255,0.35); }
  body.edit-delete .thumb.selected { outline:2px solid #c0392b; box-shadow: 0 0 0 3px rgba(192,57,43,0.35); }
  .deleteAction { background:#3a2a2a !important; color:#e09f9f !important; border:1px solid #5a3a3a !important; }
  .deleteAction:not(:disabled):hover { background:#c0392b !important; color:#fff !important; border-color:#c0392b !important; }
  .deleteAction:disabled { opacity:0.35; }
  .sep { display:inline-block; width:1px; height:20px; background:#444; margin: 0 0.2rem; }
  .status { margin-left:auto; color:#999; font-variant-numeric: tabular-nums; font-size: 0.85rem; }
  .body { flex:1; display:flex; min-height: 0; position:relative; }
  .sidebar { width: 200px; background:#252526; border-right:1px solid #333; display:flex; flex-direction:column; min-height:0; }
  .sidebar.hidden { display:none; }
  .tabs { display:flex; border-bottom:1px solid #333; }
  .tab { flex:1; padding:0.4rem; background:#2a2a2b; color:#bbb; border:none; cursor:pointer; }
  .tab.active { background:#1e1e1e; color:#fff; border-bottom: 2px solid #4a8cff; }
  .panel { flex:1; overflow:auto; padding:0.4rem; }
  .panel.hidden { display:none; }
  .thumb { position:relative; display:block; margin:0 auto 0.6rem; cursor:pointer;
           border:2px solid transparent; max-width:100%; width:fit-content;
           user-select:none; -webkit-user-select:none; }
  .thumb.active { border-color:#4a8cff; }
  .thumb.selected.active { border-color:transparent; }
  .thumbImg { display:block; background:white; max-width:100%;
              box-shadow:0 1px 3px rgba(0,0,0,0.4); -webkit-user-drag:none; }
  .thumbLabel { position:absolute; top:4px; right:4px;
                background:rgba(0,0,0,0.72); color:#fff; font-size:0.72rem;
                padding:1px 6px; border-radius:3px; pointer-events:none;
                font-variant-numeric: tabular-nums; }
  .outlineItem { padding: 0.15rem 0.3rem; cursor:pointer; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; font-size:0.85rem; }
  .outlineItem:hover { background:#333; }
  .outlineChildren { margin-left: 0.8rem; border-left:1px solid #333; padding-left:0.3rem; }
  .viewer { flex:1; overflow:auto; background:#2b2b2b; padding:1rem; }
  .pageList { display:flex; flex-direction:column; align-items:center; gap: 1rem; }
  .pageList.double { flex-flow: row wrap; justify-content:center; align-content:flex-start; gap: 0.6rem 0.6rem; }
  .pageList.single > .pageSlot:not(.current) { display:none; }
  .pageSlot { position:relative; background:white; box-shadow: 0 2px 8px rgba(0,0,0,0.4); flex: 0 0 auto; }
  .pageSlot img.page { display:block; width:100%; height:100%; }
  .pageSlot .placeholder { position:absolute; inset:0; display:flex; align-items:center; justify-content:center;
                           color:#888; font-size:0.8rem; background: repeating-linear-gradient(45deg,#f0f0f0,#f0f0f0 10px,#e6e6e6 10px,#e6e6e6 20px); }
  .selectLayer { position:absolute; inset:0; cursor:text; color:transparent; user-select:text;
                 pointer-events:auto; line-height: 1; }
  .selectLayer span { position:absolute; white-space:pre; color:transparent; user-select:text;
                      -webkit-user-select:text; transform-origin: 0 0; }
  .selectLayer span::selection { background: rgba(74,140,255,0.4); color:transparent; }
  .highlightLayer { position:absolute; inset:0; pointer-events:none; }
  .highlightLayer .hit { position:absolute; background: rgba(255,220,0,0.45); }
  .highlightLayer .hit.current { background: rgba(255,140,0,0.6); outline:1px solid orange; }
  .linkLayer { position:absolute; inset:0; pointer-events:none; }
  .linkLayer a { position:absolute; pointer-events:auto; display:block;
                 background: transparent; border: 0; cursor:pointer;
                 text-decoration:none; }
  .linkLayer a:hover { background: rgba(74,140,255,0.22); outline: 1px solid rgba(74,140,255,0.6); }
  .searchPanel, .gotoPanel { position:absolute; top: 0.5rem; right: 1rem; background:#2d2d30;
                 border:1px solid #444; border-radius:4px; padding: 0.3rem;
                 display:flex; gap:0.3rem; align-items:center; z-index: 20; }
  .searchPanel.hidden, .gotoPanel.hidden { display:none; }
  .searchPanel input[type=text], .searchPanel #searchInput {
    background:#3a3a3a; color:#ddd; border:1px solid #555; padding:0.2rem 0.4rem; width:16rem;
  }
  .gotoPanel input { background:#3a3a3a; color:#ddd; border:1px solid #555; padding:0.2rem 0.4rem; width:6rem; }
  .gotoPanel button { background:#3a3a3a; color:#ddd; border:1px solid #4a4a4a; border-radius:3px; padding:0.2rem 0.5rem; }
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
  body.presentation .ribbon, body.presentation .sidebar { display:none; }
  body.presentation .viewer { padding:0; background:#000; }
  body.organize .sidebar { display:flex; width:100%; border-right:none; }
  body.organize .sidebar.hidden { display:flex; }
  body.organize .sidebar .tabs { display:none; }
  body.organize .viewer { display:none; }
  body.organize #thumbsPanel {
    display:grid;
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
    gap:1rem; padding:1.2rem; align-content:start;
  }
  body.organize #thumbsPanel .thumb {
    width:100%; max-width:200px; margin:0 auto;
  }
  body.organize #thumbsPanel .thumbImg { width:100%; height:auto; }
  body.organize #outlinePanel, body.organize #attachmentsPanel { display:none !important; }
  body.organize .thumb { cursor:grab; }
  body.organize .thumb:active { cursor:grabbing; }
  body.organize .thumb.dragging { opacity:0.35; }
  body.organize .thumb.dropBefore { box-shadow: inset 4px 0 0 #4a8cff; }
  body.organize .thumb.dropAfter  { box-shadow: inset -4px 0 0 #4a8cff; }
  body.organize .thumbLabel { font-size:0.78rem; padding:2px 7px; }
  .dragPreview {
    position:fixed; pointer-events:none; z-index:9999;
    background:#4a8cff; color:#fff; padding:4px 10px; border-radius:999px;
    font-size:0.82rem; box-shadow:0 3px 10px rgba(0,0,0,0.5);
  }
  .dropMark { outline:3px solid #4a8cff; outline-offset:2px; }
  body.presentation .pageList { gap:0; }
  body.dark .pageSlot img.page { filter: invert(0.92) hue-rotate(180deg); }
  body.dark .pageSlot { background:#222; }
  body.dark .selectLayer span::selection { background: rgba(74,140,255,0.5); }
  .attachmentItem { display:flex; gap:0.4rem; align-items:center; padding:0.3rem; border-bottom:1px solid #333; font-size:0.82rem; }
  .attachmentItem .name { flex:1; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .attachmentItem .size { color:#888; font-variant-numeric: tabular-nums; }
  .attachmentItem button { background:#3a3a3a; color:#ddd; border:1px solid #4a4a4a; border-radius:3px; padding:0.1rem 0.4rem; cursor:pointer; }
  .attachmentItem button:hover { background:#4a4a4a; }
  .attachEmpty { color:#777; padding:0.4rem; font-size:0.85rem; }
  .propsOverlay { position:fixed; inset:0; background: rgba(0,0,0,0.5); display:flex; align-items:center; justify-content:center; z-index:40; }
  .propsOverlay.hidden { display:none; }
  .propsModal { background:#2d2d30; border:1px solid #444; border-radius:6px; min-width: 28rem; max-width:40rem; max-height:80vh; display:flex; flex-direction:column; }
  .propsHeader { display:flex; align-items:center; border-bottom:1px solid #444; padding: 0.1rem 0.2rem; }
  .propsTabs { display:flex; flex:1; }
  .propsTab { background:transparent; color:#bbb; border:none; padding:0.45rem 0.8rem; cursor:pointer; font:inherit; }
  .propsTab.active { color:#fff; border-bottom: 2px solid #4a8cff; }
  .propsClose { background:transparent; color:#bbb; border:none; padding:0.3rem 0.6rem; cursor:pointer; font-size:1rem; }
  .propsPanel { padding: 0.8rem 1rem; overflow:auto; }
  .propsPanel.hidden { display:none; }
  .propsRow { display:flex; gap:0.6rem; padding:0.25rem 0; font-size:0.88rem; }
  .propsRow .k { width: 10rem; color:#888; }
  .propsRow .v { flex:1; color:#ddd; word-break: break-word; }
  .propsRow .v.no { color:#777; }
  .propsRow .v.yes { color:#9fe09f; }
  .propsRow .v.bad { color:#e09f9f; }
`;
document.head.appendChild(style);

const statusEl = document.getElementById("status") as HTMLSpanElement;
const viewer = document.getElementById("viewer") as HTMLDivElement;
const pageList = document.getElementById("pageList") as HTMLDivElement;
const zoomSelect = document.getElementById("zoomSelect") as HTMLSelectElement;
const zoomCustomOpt = document.getElementById("zoomCustomOpt") as HTMLOptionElement;
const ZOOM_PRESETS = [0.1, 0.5, 0.8, 1, 1.2, 1.5, 2];
function syncZoomUi() {
  const pct = Math.round(zoom * 100);
  zoomCustomOpt.textContent = `${pct}%`;
  const hit = ZOOM_PRESETS.find((v) => Math.abs(v - zoom) < 0.005);
  zoomSelect.value = hit !== undefined ? String(hit) : "custom";
}
const fitSelect = document.getElementById("fitSelect") as HTMLSelectElement;
const viewModeSelect = document.getElementById("viewModeSelect") as HTMLSelectElement;
const pageInput = document.getElementById("pageInput") as HTMLInputElement;
const pageTotal = document.getElementById("pageTotal") as HTMLSpanElement;
const searchPanel = document.getElementById("searchPanel") as HTMLDivElement;
const searchInput = document.getElementById("searchInput") as HTMLInputElement;
const searchCase = document.getElementById("searchCase") as HTMLInputElement;
const searchCount = document.getElementById("searchCount") as HTMLSpanElement;
const gotoPanel = document.getElementById("gotoPanel") as HTMLDivElement;
const gotoInput = document.getElementById("gotoInput") as HTMLInputElement;
const thumbsPanel = document.getElementById("thumbsPanel") as HTMLDivElement;
const outlinePanel = document.getElementById("outlinePanel") as HTMLDivElement;
const recentMenu = document.getElementById("recentMenu") as HTMLDivElement;
const dropOverlay = document.getElementById("dropOverlay") as HTMLDivElement;
const backBtn = document.getElementById("backBtn") as HTMLButtonElement;
const forwardBtn = document.getElementById("forwardBtn") as HTMLButtonElement;
const attachmentsPanel = document.getElementById("attachmentsPanel") as HTMLDivElement;
const propsOverlay = document.getElementById("propsOverlay") as HTMLDivElement;
const propsInfoPanel = document.getElementById("propsInfo") as HTMLDivElement;
const propsPermsPanel = document.getElementById("propsPerms") as HTMLDivElement;

type Slot = {
  wrap: HTMLDivElement;
  img: HTMLImageElement;
  selectLayer: HTMLDivElement;
  highlightLayer: HTMLDivElement;
  linkLayer: HTMLDivElement;
  placeholder: HTMLDivElement;
  loaded: boolean;
  renderingZoom: number | null;
  lastZoom: number | null;
};

let currentDoc: string | null = null;
let currentDocPath: string | null = null;
type RibbonTab = "reader" | "edit" | "organize";
type EditMode = "select" | "delete";
let ribbonTab: RibbonTab = "reader";
let editMode: EditMode = "select";
const selectedPages = new Set<number>();
let selectionAnchor: number | null = null;
let pageCount = 0;
let pageSizes: PageSize[] = [];
let outline: OutlineNode[] = [];
let currentPage = 0;
let zoom = 1.0;
let fitMode: FitMode = "page";
let viewMode: ViewMode = "continuous";
let hits: SearchHit[] = [];
let currentHit = -1;
let docRotation = 0;
const pageRotation: Map<number, number> = new Map();
let darkMode = false;
const textRunsCache: Map<number, TextRun[]> = new Map();
const linksCache: Map<number, PageLink[]> = new Map();
const thumbEls: HTMLImageElement[] = [];
const thumbWraps: HTMLElement[] = [];
let slots: Slot[] = [];
let observer: IntersectionObserver | null = null;
const renderQueue: Set<number> = new Set();
let renderRunning = false;
const backStack: number[] = [];
const forwardStack: number[] = [];

function setStatus(s: string) {
  statusEl.textContent = s;
}

function updateChrome() {
  if (!currentDoc) {
    pageInput.value = "1";
    pageTotal.textContent = "/ 0";
    syncZoomUi();
    backBtn.disabled = true;
    forwardBtn.disabled = true;
    return;
  }
  pageInput.value = String(currentPage + 1);
  pageInput.max = String(pageCount);
  pageTotal.textContent = `/ ${pageCount}`;
  syncZoomUi();
  thumbWraps.forEach((w, i) => w.classList.toggle("active", i === currentPage));
  backBtn.disabled = backStack.length === 0;
  forwardBtn.disabled = forwardStack.length === 0;
}

function computeZoom(): number {
  if (!currentDoc || !pageSizes[currentPage]) return zoom;
  const rs = rotatedSize(currentPage);
  if (rs.w === 0) return zoom;
  const vw = viewer.clientWidth - 32;
  const vh = viewer.clientHeight - 32;
  if (vw <= 0 || vh <= 0) return zoom;
  const cols = viewMode === "double" ? 2 : 1;
  const colW = (vw - (cols - 1) * 10) / cols;
  switch (fitMode) {
    case "width":
      return clampZoom(colW / rs.w);
    case "page":
      return clampZoom(Math.min(colW / rs.w, vh / rs.h));
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

function effectiveRotation(idx: number): number {
  return ((docRotation + (pageRotation.get(idx) || 0)) % 4 + 4) % 4;
}

function rotatedSize(idx: number): { w: number; h: number } {
  const sz = pageSizes[idx];
  if (!sz) return { w: 0, h: 0 };
  const rot = effectiveRotation(idx);
  if (rot === 1 || rot === 3) return { w: sz.height_pt, h: sz.width_pt };
  return { w: sz.width_pt, h: sz.height_pt };
}

function makeSlot(pageIdx: number): Slot {
  const wrap = document.createElement("div");
  wrap.className = "pageSlot";
  wrap.dataset.page = String(pageIdx);
  const img = document.createElement("img");
  img.className = "page";
  img.alt = `Page ${pageIdx + 1}`;
  const selectLayer = document.createElement("div");
  selectLayer.className = "selectLayer";
  const highlightLayer = document.createElement("div");
  highlightLayer.className = "highlightLayer";
  const linkLayer = document.createElement("div");
  linkLayer.className = "linkLayer";
  const placeholder = document.createElement("div");
  placeholder.className = "placeholder";
  placeholder.textContent = `${pageIdx + 1}`;
  wrap.appendChild(placeholder);
  wrap.appendChild(img);
  wrap.appendChild(selectLayer);
  wrap.appendChild(highlightLayer);
  wrap.appendChild(linkLayer);
  return {
    wrap,
    img,
    selectLayer,
    highlightLayer,
    linkLayer,
    placeholder,
    loaded: false,
    renderingZoom: null,
    lastZoom: null,
  };
}

function rebuildSlots() {
  pageList.innerHTML = "";
  slots = [];
  for (let i = 0; i < pageCount; i++) {
    const s = makeSlot(i);
    pageList.appendChild(s.wrap);
    slots.push(s);
  }
  applyViewMode();
  sizeSlots();
  setupObserver();
}

function applyViewMode() {
  pageList.classList.remove("single", "continuous", "double");
  pageList.classList.add(viewMode);
  if (viewMode === "single") {
    slots.forEach((s, i) => s.wrap.classList.toggle("current", i === currentPage));
  }
}

function sizeSlots() {
  if (slots.length === 0) return;
  zoom = computeZoom();
  for (let i = 0; i < slots.length; i++) {
    const rs = rotatedSize(i);
    if (rs.w === 0) continue;
    const w = Math.round(rs.w * zoom);
    const h = Math.round(rs.h * zoom);
    const slot = slots[i];
    slot.wrap.style.width = `${w}px`;
    slot.wrap.style.height = `${h}px`;
    slot.img.width = w;
    slot.img.height = h;
  }
}

const EVICT_DIST = 10;

function evictSlot(idx: number) {
  const slot = slots[idx];
  if (!slot || !slot.loaded) return;
  // Drop decoded bitmap so browser frees memory. The placeholder keeps the
  // slot sized; re-render happens when the slot intersects again.
  slot.img.removeAttribute("src");
  slot.img.width = 0;
  slot.img.height = 0;
  slot.selectLayer.innerHTML = "";
  slot.highlightLayer.innerHTML = "";
  slot.linkLayer.innerHTML = "";
  slot.loaded = false;
  slot.lastZoom = null;
  renderQueue.delete(idx);
}

function evictFarSlots() {
  for (let i = 0; i < slots.length; i++) {
    if (Math.abs(i - currentPage) > EVICT_DIST) evictSlot(i);
  }
}

function setupObserver() {
  if (observer) observer.disconnect();
  observer = new IntersectionObserver(
    (entries) => {
      for (const e of entries) {
        const idx = parseInt((e.target as HTMLElement).dataset.page || "-1", 10);
        if (idx < 0) continue;
        if (e.isIntersecting) {
          scheduleRender(idx);
          if (viewMode === "continuous" || viewMode === "double") {
            updateCurrentFromScroll();
          }
        } else if (Math.abs(idx - currentPage) > EVICT_DIST) {
          evictSlot(idx);
        }
      }
    },
    { root: viewer, rootMargin: "200px 0px", threshold: 0.01 },
  );
  for (const s of slots) observer.observe(s.wrap);
}

function scheduleRender(idx: number) {
  if (!currentDoc) return;
  const slot = slots[idx];
  if (!slot) return;
  if (slot.loaded && slot.lastZoom === zoom) return;
  renderQueue.add(idx);
  pumpRenderQueue();
}

const MAX_RENDER_DIST = 6;

function pruneRenderQueue() {
  // Drop entries far from viewport — they were likely queued by a scroll
  // burst and the user has already moved past them.
  for (const idx of renderQueue) {
    if (Math.abs(idx - currentPage) > MAX_RENDER_DIST) {
      renderQueue.delete(idx);
    }
  }
}

async function pumpRenderQueue() {
  if (renderRunning) return;
  renderRunning = true;
  try {
    while (renderQueue.size > 0) {
      pruneRenderQueue();
      const nearest = pickNearest();
      if (nearest === null) break;
      renderQueue.delete(nearest);
      await renderSlot(nearest);
    }
  } finally {
    renderRunning = false;
  }
}

function pickNearest(): number | null {
  let best: number | null = null;
  let bestDist = Infinity;
  for (const idx of renderQueue) {
    const d = Math.abs(idx - currentPage);
    if (d < bestDist) {
      bestDist = d;
      best = idx;
    }
  }
  return best;
}

async function renderSlot(idx: number) {
  if (!currentDoc) return;
  const slot = slots[idx];
  if (!slot) return;
  const docAtStart = currentDoc;
  const zoomAtStart = zoom;
  slot.renderingZoom = zoomAtStart;
  try {
    const res = await invoke<Rendered>("render_page", {
      document: docAtStart,
      page: idx,
      zoom: zoomAtStart,
      rotation: effectiveRotation(idx),
    });
    if (docAtStart !== currentDoc) return;
    slot.img.src = res.data_url;
    slot.img.width = res.width;
    slot.img.height = res.height;
    slot.wrap.style.width = `${res.width}px`;
    slot.wrap.style.height = `${res.height}px`;
    slot.placeholder.style.display = "none";
    slot.loaded = true;
    slot.lastZoom = zoomAtStart;
    await ensureTextLayer(idx);
    renderHighlightsFor(idx);
    await ensureLinkLayer(idx);
  } catch (e) {
    setStatus(`Render error p${idx + 1}: ${e}`);
  } finally {
    slot.renderingZoom = null;
  }
}

async function ensureTextLayer(idx: number) {
  if (!currentDoc) return;
  const slot = slots[idx];
  if (!slot) return;
  let runs = textRunsCache.get(idx);
  if (!runs) {
    try {
      runs = await invoke<TextRun[]>("page_text_layout_cmd", {
        document: currentDoc,
        page: idx,
      });
      textRunsCache.set(idx, runs);
    } catch {
      runs = [];
    }
  }
  const sz = pageSizes[idx];
  if (!sz) return;
  const rot = effectiveRotation(idx);
  slot.selectLayer.innerHTML = "";
  for (const run of runs) {
    const box = rotateRectToSlot(run.rect_pt, rot, sz, slot.img.width, slot.img.height);
    if (box.width <= 0 || box.height <= 0) continue;
    const span = document.createElement("span");
    span.textContent = run.text;
    span.style.left = `${box.left}px`;
    span.style.top = `${box.top}px`;
    span.style.fontSize = `${box.height}px`;
    span.style.height = `${box.height}px`;
    span.style.width = `${box.width}px`;
    slot.selectLayer.appendChild(span);
  }
}

function rotateRectToSlot(
  r: RectPt,
  rot: number,
  sz: PageSize,
  imgW: number,
  imgH: number,
): { left: number; top: number; width: number; height: number } {
  const w0 = sz.width_pt;
  const h0 = sz.height_pt;
  let nx0 = r.x0, ny0 = r.y0, nx1 = r.x1, ny1 = r.y1;
  let dw = w0, dh = h0;
  switch (rot & 3) {
    case 0:
      break;
    case 1:
      nx0 = h0 - r.y1; ny0 = r.x0; nx1 = h0 - r.y0; ny1 = r.x1;
      dw = h0; dh = w0;
      break;
    case 2:
      nx0 = w0 - r.x1; ny0 = h0 - r.y1; nx1 = w0 - r.x0; ny1 = h0 - r.y0;
      break;
    case 3:
      nx0 = r.y0; ny0 = w0 - r.x1; nx1 = r.y1; ny1 = w0 - r.x0;
      dw = h0; dh = w0;
      break;
  }
  const sx = imgW / dw;
  const sy = imgH / dh;
  return {
    left: nx0 * sx,
    top: ny0 * sy,
    width: (nx1 - nx0) * sx,
    height: (ny1 - ny0) * sy,
  };
}

async function ensureLinkLayer(idx: number) {
  if (!currentDoc) return;
  const slot = slots[idx];
  if (!slot) return;
  slot.linkLayer.innerHTML = "";
  let links = linksCache.get(idx);
  if (!links) {
    try {
      links = await invoke<PageLink[]>("page_links_cmd", {
        document: currentDoc,
        page: idx,
      });
      linksCache.set(idx, links);
    } catch {
      links = [];
    }
  }
  const sz = pageSizes[idx];
  if (!sz || links.length === 0) return;
  const rot = effectiveRotation(idx);
  for (const link of links) {
    const box = rotateRectToSlot(link.rect_pt, rot, sz, slot.img.width, slot.img.height);
    if (box.width <= 0 || box.height <= 0) continue;
    const a = document.createElement("a");
    a.style.left = `${box.left}px`;
    a.style.top = `${box.top}px`;
    a.style.width = `${box.width}px`;
    a.style.height = `${box.height}px`;
    if (link.target.kind === "Page") {
      const targetPage = link.target.value;
      a.title = `Page ${targetPage + 1}`;
      a.href = `#page-${targetPage + 1}`;
      a.addEventListener("click", (ev) => {
        ev.preventDefault();
        navigateTo(targetPage, { recordHistory: true });
      });
    } else {
      const url = link.target.value;
      a.title = url;
      a.href = url;
      a.addEventListener("click", async (ev) => {
        ev.preventDefault();
        try {
          await invoke("open_external", { url });
        } catch (err) {
          setStatus(`Open link error: ${err}`);
        }
      });
    }
    slot.linkLayer.appendChild(a);
  }
}

function renderHighlightsFor(idx: number) {
  const slot = slots[idx];
  if (!slot) return;
  const sz = pageSizes[idx];
  if (!sz) return;
  slot.highlightLayer.innerHTML = "";
  const rot = effectiveRotation(idx);
  hits.forEach((hit, hIdx) => {
    if (hit.page !== idx) return;
    for (const r of hit.rects_pt) {
      const box = rotateRectToSlot(r, rot, sz, slot.img.width, slot.img.height);
      if (box.width <= 0 || box.height <= 0) continue;
      const el = document.createElement("div");
      el.className = "hit" + (hIdx === currentHit ? " current" : "");
      el.style.left = `${box.left}px`;
      el.style.top = `${box.top}px`;
      el.style.width = `${box.width}px`;
      el.style.height = `${box.height}px`;
      slot.highlightLayer.appendChild(el);
    }
  });
}

function renderAllHighlights() {
  for (let i = 0; i < slots.length; i++) renderHighlightsFor(i);
}

function invalidateRenders() {
  for (const s of slots) {
    s.loaded = false;
    s.lastZoom = null;
    s.placeholder.style.display = "";
    s.img.removeAttribute("src");
    s.linkLayer.innerHTML = "";
  }
  renderQueue.clear();
}

function reflow() {
  if (!currentDoc) return;
  invalidateRenders();
  sizeSlots();
  if (viewMode === "single") {
    applyViewMode();
    scheduleRender(currentPage);
    const slot = slots[currentPage];
    if (slot) slot.wrap.scrollIntoView({ block: "start" });
  } else {
    for (const [i, slot] of slots.entries()) {
      const rect = slot.wrap.getBoundingClientRect();
      const vRect = viewer.getBoundingClientRect();
      const visible = rect.bottom > vRect.top - 400 && rect.top < vRect.bottom + 400;
      if (visible) scheduleRender(i);
    }
  }
  updateChrome();
}

function updateCurrentFromScroll() {
  if (slots.length === 0) return;
  const vRect = viewer.getBoundingClientRect();
  const midY = vRect.top + vRect.height / 2;
  let best = currentPage;
  let bestDist = Infinity;
  for (let i = 0; i < slots.length; i++) {
    const r = slots[i].wrap.getBoundingClientRect();
    const c = (r.top + r.bottom) / 2;
    const d = Math.abs(c - midY);
    if (d < bestDist) {
      bestDist = d;
      best = i;
    }
  }
  if (best !== currentPage) {
    currentPage = best;
    updateChrome();
  }
}

function navigateTo(idx: number, opts: { recordHistory?: boolean } = {}) {
  const clamped = Math.max(0, Math.min(pageCount - 1, idx));
  if (clamped === currentPage) {
    updateChrome();
    return;
  }
  if (opts.recordHistory) {
    backStack.push(currentPage);
    if (backStack.length > 200) backStack.shift();
    forwardStack.length = 0;
  }
  currentPage = clamped;
  if (viewMode === "single") {
    applyViewMode();
    scheduleRender(clamped);
  } else {
    const slot = slots[clamped];
    if (slot) slot.wrap.scrollIntoView({ block: "start", behavior: "auto" });
    scheduleRender(clamped);
  }
  evictFarSlots();
  updateChrome();
}

function goToPage(idx: number) {
  navigateTo(idx, { recordHistory: true });
}

function historyBack() {
  if (backStack.length === 0) return;
  const target = backStack.pop()!;
  forwardStack.push(currentPage);
  navigateTo(target, { recordHistory: false });
}

function historyForward() {
  if (forwardStack.length === 0) return;
  const target = forwardStack.pop()!;
  backStack.push(currentPage);
  navigateTo(target, { recordHistory: false });
}

type OpenErr = { kind: "PasswordRequired" | "PasswordIncorrect" | "Other"; value?: string };

async function tryOpen(
  bytes: Uint8Array,
  password: string | null,
): Promise<OpenedDocument> {
  return await invoke<OpenedDocument>("open_pdf", {
    bytes: Array.from(bytes),
    password,
  });
}

async function openFromBytes(bytes: Uint8Array, label: string) {
  setStatus(`Opening ${label}…`);
  let doc: OpenedDocument | null = null;
  let password: string | null = null;
  let attempts = 0;
  while (doc === null) {
    try {
      doc = await tryOpen(bytes, password);
    } catch (e) {
      const err = e as OpenErr | string;
      const kind = typeof err === "object" ? err.kind : "Other";
      if (kind === "PasswordRequired" || kind === "PasswordIncorrect") {
        if (attempts >= 3) {
          setStatus("Too many password attempts");
          return;
        }
        const prompt = kind === "PasswordIncorrect" ? "Password incorrect. Try again:" : "Password required:";
        const input = window.prompt(prompt);
        if (input === null) {
          setStatus("Open cancelled");
          return;
        }
        password = input;
        attempts += 1;
        continue;
      }
      const msg = typeof err === "string" ? err : err.value ?? "unknown error";
      setStatus(`Error: ${msg}`);
      return;
    }
  }
  applyOpenedDoc(doc, label);
}

function applyOpenedDoc(doc: OpenedDocument, label: string) {
  try {
    const prior = currentDoc;
    currentDoc = doc.id;
    currentDocPath = label.endsWith(".pdf") ? label : null;
    clearSelection();
    if (prior && prior !== doc.id) {
      invoke("close_pdf", { document: prior }).catch(() => {});
    }
    pageCount = doc.page_count;
    pageSizes = doc.page_sizes;
    outline = doc.outline;
    currentPage = 0;
    hits = [];
    currentHit = -1;
    docRotation = 0;
    pageRotation.clear();
    textRunsCache.clear();
    linksCache.clear();
    backStack.length = 0;
    forwardStack.length = 0;
    searchCount.textContent = "";
    renderOutline();
    rebuildSlots();
    if (viewMode === "single") navigateTo(0, { recordHistory: false });
    else scheduleRender(0);
    renderThumbsShell();
    loadAttachments();
    setStatus(`${label} — ${pageCount} pages`);
    updateChrome();
  } catch (e) {
    setStatus(`Error: ${e}`);
  }
}

async function openByPath(path: string) {
  const t0 = performance.now();
  setStatus(`Opening ${path}…`);
  let doc: OpenedDocument | null = null;
  let password: string | null = null;
  let attempts = 0;
  while (doc === null) {
    try {
      doc = await invoke<OpenedDocument>("open_pdf_path", { path, password });
    } catch (e) {
      const err = e as OpenErr | string;
      const kind = typeof err === "object" ? err.kind : "Other";
      if (kind === "PasswordRequired" || kind === "PasswordIncorrect") {
        if (attempts >= 3) { setStatus("Too many password attempts"); return; }
        const prompt = kind === "PasswordIncorrect" ? "Password incorrect. Try again:" : "Password required:";
        const input = window.prompt(prompt);
        if (input === null) { setStatus("Open cancelled"); return; }
        password = input; attempts += 1; continue;
      }
      const msg = typeof err === "string" ? err : err.value ?? "unknown error";
      setStatus(`Error: ${msg}`); return;
    }
  }
  console.info(`open_pdf_path round-trip: ${(performance.now() - t0).toFixed(1)}ms`);
  applyOpenedDoc(doc, path);
  try { await invoke("push_recent", { path }); } catch {}
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

const sidebarEl = document.getElementById("sidebar") as HTMLElement;
function toggleSidebar() {
  const becomingVisible = sidebarEl.classList.contains("hidden");
  sidebarEl.classList.toggle("hidden");
  if (becomingVisible) {
    const activeTab = document.querySelector(".tab.active") as HTMLElement | null;
    const tab = activeTab?.dataset.tab;
    if (tab === "thumbs") armThumbObserver();
  }
}
document.getElementById("sidebarBtn")!.addEventListener("click", toggleSidebar);

backBtn.addEventListener("click", historyBack);
forwardBtn.addEventListener("click", historyForward);

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
  reflow();
});
document.getElementById("zoomOut")!.addEventListener("click", () => {
  fitMode = "custom";
  fitSelect.value = "custom";
  zoom = clampZoom(zoom - 0.1);
  reflow();
});
fitSelect.addEventListener("change", () => {
  fitMode = fitSelect.value as FitMode;
  reflow();
});
fitSelect.value = fitMode;
zoomSelect.addEventListener("change", () => {
  const v = zoomSelect.value;
  if (v === "custom") { syncZoomUi(); return; }
  const z = parseFloat(v);
  if (!isFinite(z)) return;
  fitMode = "custom";
  fitSelect.value = "custom";
  zoom = clampZoom(z);
  reflow();
});

viewModeSelect.addEventListener("change", () => {
  viewMode = viewModeSelect.value as ViewMode;
  applyViewMode();
  reflow();
  if (viewMode === "single") navigateTo(currentPage, { recordHistory: false });
});
viewModeSelect.value = viewMode;

function rotate(delta: number, docWide: boolean) {
  if (!currentDoc) return;
  if (docWide) {
    docRotation = (((docRotation + delta) % 4) + 4) % 4;
  } else {
    const cur = pageRotation.get(currentPage) || 0;
    pageRotation.set(currentPage, (((cur + delta) % 4) + 4) % 4);
  }
  reflow();
}

document.getElementById("rotLeft")!.addEventListener("click", (e) => rotate(-1, e.shiftKey));
document.getElementById("rotRight")!.addEventListener("click", (e) => rotate(1, e.shiftKey));

function setDarkMode(on: boolean) {
  darkMode = on;
  document.body.classList.toggle("dark", darkMode);
}
document.getElementById("darkToggle")!.addEventListener("click", () => setDarkMode(!darkMode));

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
      reflow();
      return;
    }
    if (viewMode !== "single") return;
    const atTop = viewer.scrollTop <= 0;
    const atBottom = viewer.scrollTop + viewer.clientHeight >= viewer.scrollHeight - 1;
    if (e.deltaY < 0 && atTop && currentPage > 0) {
      e.preventDefault();
      goToPage(currentPage - 1);
      viewer.scrollTop = viewer.scrollHeight;
    } else if (e.deltaY > 0 && atBottom && currentPage < pageCount - 1) {
      e.preventDefault();
      goToPage(currentPage + 1);
      viewer.scrollTop = 0;
    }
  },
  { passive: false },
);

viewer.addEventListener("scroll", () => {
  if (viewMode !== "single") updateCurrentFromScroll();
});

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

let thumbObserver: IntersectionObserver | null = null;
const thumbLoaded = new Set<number>();
const thumbInFlight = new Set<number>();

function renderThumbsShell() {
  thumbsPanel.innerHTML = "";
  thumbEls.length = 0;
  thumbLoaded.clear();
  thumbInFlight.clear();
  if (thumbObserver) {
    thumbObserver.disconnect();
    thumbObserver = null;
  }
  thumbWraps.length = 0;
  for (let i = 0; i < pageCount; i++) {
    const wrap = document.createElement("div");
    wrap.className = "thumb";
    wrap.dataset.page = String(i);
    wrap.draggable = false;
    const img = document.createElement("img");
    img.className = "thumbImg";
    img.dataset.page = String(i);
    img.draggable = false;
    const sz = pageSizes[i];
    if (sz) {
      const ratio = sz.height_pt / Math.max(sz.width_pt, 1);
      wrap.style.width = "170px";
      img.style.width = "170px";
      img.style.height = `${Math.round(170 * ratio)}px`;
    }
    const label = document.createElement("div");
    label.className = "thumbLabel";
    label.textContent = `${i + 1}`;
    wrap.appendChild(img);
    wrap.appendChild(label);
    thumbsPanel.appendChild(wrap);
    thumbEls.push(img);
    thumbWraps.push(wrap);
  }
  if (!sidebarEl.classList.contains("hidden")) {
    const activeTab = document.querySelector(".tab.active") as HTMLElement | null;
    if (activeTab?.dataset.tab === "thumbs") armThumbObserver();
  }
  refreshSelectionUi();
}

function armThumbObserver() {
  if (thumbObserver || thumbEls.length === 0) return;
  thumbObserver = new IntersectionObserver(
    (entries) => {
      for (const e of entries) {
        if (!e.isIntersecting) continue;
        const i = parseInt((e.target as HTMLElement).dataset.page || "-1", 10);
        if (i < 0) continue;
        loadThumb(i);
      }
    },
    { root: thumbsPanel, rootMargin: "150px 0px", threshold: 0.01 },
  );
  for (const img of thumbEls) thumbObserver.observe(img);
}

async function loadThumb(i: number) {
  if (!currentDoc) return;
  if (thumbLoaded.has(i) || thumbInFlight.has(i)) return;
  const doc = currentDoc;
  const sz = pageSizes[i];
  if (!sz) return;
  thumbInFlight.add(i);
  try {
    const res = await invoke<Rendered>("render_thumbnail", {
      document: doc,
      page: i,
      maxWidth: 170,
      pageWidthPt: sz.width_pt,
    });
    if (doc !== currentDoc) return;
    thumbEls[i].src = res.data_url;
    thumbLoaded.add(i);
  } catch {
  } finally {
    thumbInFlight.delete(i);
  }
}

document.querySelectorAll(".tab").forEach((el) => {
  el.addEventListener("click", () => {
    document.querySelectorAll(".tab").forEach((t) => t.classList.remove("active"));
    el.classList.add("active");
    const tab = (el as HTMLElement).dataset.tab;
    thumbsPanel.classList.toggle("hidden", tab !== "thumbs");
    outlinePanel.classList.toggle("hidden", tab !== "outline");
    attachmentsPanel.classList.toggle("hidden", tab !== "attachments");
    if (tab === "thumbs" && !sidebarEl.classList.contains("hidden")) armThumbObserver();
  });
});

async function loadAttachments() {
  attachmentsPanel.innerHTML = "";
  if (!currentDoc) return;
  const doc = currentDoc;
  let list: AttachmentInfo[] = [];
  try {
    list = await invoke<AttachmentInfo[]>("list_attachments_cmd", { document: doc });
  } catch {
    list = [];
  }
  if (doc !== currentDoc) return;
  if (list.length === 0) {
    const empty = document.createElement("div");
    empty.className = "attachEmpty";
    empty.textContent = "No attachments";
    attachmentsPanel.appendChild(empty);
    return;
  }
  for (const a of list) {
    const row = document.createElement("div");
    row.className = "attachmentItem";
    const name = document.createElement("span");
    name.className = "name";
    name.title = a.name;
    name.textContent = a.name || `Attachment ${a.index + 1}`;
    const size = document.createElement("span");
    size.className = "size";
    size.textContent = formatSize(a.size);
    const btn = document.createElement("button");
    btn.textContent = "Save";
    btn.addEventListener("click", async () => {
      const defaultName = a.name || `attachment-${a.index + 1}`;
      const dest = await save({ defaultPath: defaultName });
      if (!dest || typeof dest !== "string") return;
      try {
        await invoke("save_attachment_cmd", { document: doc, index: a.index, path: dest });
        setStatus(`Saved attachment: ${dest}`);
      } catch (e) {
        setStatus(`Save error: ${e}`);
      }
    });
    row.appendChild(name);
    row.appendChild(size);
    row.appendChild(btn);
    attachmentsPanel.appendChild(row);
  }
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

async function openProperties() {
  if (!currentDoc) return;
  propsOverlay.classList.remove("hidden");
  propsInfoPanel.innerHTML = '<div style="color:#777;">Loading…</div>';
  propsPermsPanel.innerHTML = "";
  try {
    const [md, perms] = await Promise.all([
      invoke<DocMetadata>("document_metadata_cmd", { document: currentDoc }),
      invoke<DocPermissions>("document_permissions_cmd", { document: currentDoc }),
    ]);
    renderPropsInfo(md);
    renderPropsPerms(perms);
  } catch (e) {
    propsInfoPanel.innerHTML = `<div style="color:#e09f9f;">Error: ${e}</div>`;
  }
}

function renderPropsInfo(md: DocMetadata) {
  const rows: [string, string | null][] = [
    ["Title", md.title],
    ["Author", md.author],
    ["Subject", md.subject],
    ["Keywords", md.keywords],
    ["Creator", md.creator],
    ["Producer", md.producer],
    ["Created", md.creation_date],
    ["Modified", md.modification_date],
  ];
  propsInfoPanel.innerHTML = "";
  for (const [k, v] of rows) {
    const row = document.createElement("div");
    row.className = "propsRow";
    const key = document.createElement("div");
    key.className = "k";
    key.textContent = k;
    const val = document.createElement("div");
    val.className = "v" + (v ? "" : " no");
    val.textContent = v || "—";
    row.appendChild(key);
    row.appendChild(val);
    propsInfoPanel.appendChild(row);
  }
}

function renderPropsPerms(p: DocPermissions) {
  const rows: [string, string | boolean][] = [
    ["Security handler", p.security_handler],
    ["Print (high quality)", p.can_print_high_quality],
    ["Print (low quality)", p.can_print_low_quality],
    ["Modify content", p.can_modify_content],
    ["Extract text/graphics", p.can_extract_text_and_graphics],
    ["Annotate", p.can_annotate],
    ["Fill forms", p.can_fill_forms],
    ["Assemble", p.can_assemble],
  ];
  propsPermsPanel.innerHTML = "";
  for (const [k, v] of rows) {
    const row = document.createElement("div");
    row.className = "propsRow";
    const key = document.createElement("div");
    key.className = "k";
    key.textContent = k;
    const val = document.createElement("div");
    if (typeof v === "boolean") {
      val.className = "v " + (v ? "yes" : "bad");
      val.textContent = v ? "Allowed" : "Denied";
    } else {
      val.className = "v";
      val.textContent = v;
    }
    row.appendChild(key);
    row.appendChild(val);
    propsPermsPanel.appendChild(row);
  }
}

function closeProperties() {
  propsOverlay.classList.add("hidden");
}

type DocSnapshot = {
  page_count: number;
  page_sizes: PageSize[];
  outline: OutlineNode[];
};

function applyDocSnapshot(snap: DocSnapshot) {
  pageCount = snap.page_count;
  pageSizes = snap.page_sizes;
  outline = snap.outline;
  pageRotation.clear();
  if (currentPage >= pageCount) currentPage = Math.max(0, pageCount - 1);
  textRunsCache.clear();
  linksCache.clear();
  hits = [];
  currentHit = -1;
  searchCount.textContent = "";
  renderOutline();
  rebuildSlots();
  if (viewMode === "single") navigateTo(currentPage, { recordHistory: false });
  else scheduleRender(currentPage);
  renderThumbsShell();
  updateChrome();
}

function clearSelection() {
  selectedPages.clear();
  selectionAnchor = null;
  refreshSelectionUi();
}

function refreshSelectionUi() {
  for (const w of thumbWraps) {
    const idx = parseInt(w.dataset.page || "-1", 10);
    w.classList.toggle("selected", selectedPages.has(idx));
  }
  const btn = document.getElementById("deleteSelectionBtn") as HTMLButtonElement | null;
  if (btn) {
    btn.disabled = selectedPages.size === 0;
    btn.textContent = selectedPages.size > 0 ? `🗑 Delete ${selectedPages.size} page${selectedPages.size === 1 ? "" : "s"}` : "🗑 Delete selected";
  }
  const ex = document.getElementById("extractSelectionBtn") as HTMLButtonElement | null;
  if (ex) {
    ex.disabled = selectedPages.size === 0;
    ex.textContent = selectedPages.size > 0 ? `⬇ Extract ${selectedPages.size} page${selectedPages.size === 1 ? "" : "s"}` : "⬇ Extract selected";
  }
  if (modeHint) {
    const base = selectedPages.size === 0
      ? "Click / Ctrl+click / Shift+click thumbnails to select pages"
      : `${selectedPages.size} selected`;
    modeHint.textContent = editMode === "delete"
      ? `⚠ Delete mode — selected pages will be removed. ${base}`
      : base;
  }
}

function handleThumbSelect(i: number, ev: MouseEvent | PointerEvent) {
  if (ev.shiftKey && selectionAnchor !== null) {
    const [a, b] = selectionAnchor <= i ? [selectionAnchor, i] : [i, selectionAnchor];
    selectedPages.clear();
    for (let k = a; k <= b; k++) selectedPages.add(k);
    goToPage(i);
  } else if (ev.ctrlKey || ev.metaKey) {
    if (selectedPages.has(i)) selectedPages.delete(i);
    else selectedPages.add(i);
    selectionAnchor = i;
  } else {
    selectedPages.clear();
    selectedPages.add(i);
    selectionAnchor = i;
    goToPage(i);
  }
  refreshSelectionUi();
}

async function deleteSelectedPages() {
  if (!currentDoc || selectedPages.size === 0) return;
  const pages = [...selectedPages].sort((a, b) => a - b);
  if (pages.length >= pageCount) {
    setStatus("Can't delete every page");
    return;
  }
  const label = pages.length === 1
    ? `page ${pages[0] + 1}`
    : `${pages.length} pages (${pages[0] + 1}–${pages[pages.length - 1] + 1})`;
  if (editMode === "select") {
    const ok = await confirmDialog(
      `Are you sure you want to delete all selected pages?\n\n(${label} — this can only be undone by not saving.)`,
      { title: "Delete pages", kind: "warning", okLabel: "Delete", cancelLabel: "Cancel" },
    );
    if (!ok) return;
  }
  setStatus(`Deleting ${label}…`);
  try {
    const snap = await invoke<DocSnapshot>("delete_pages", { document: currentDoc, pages });
    clearSelection();
    applyDocSnapshot(snap);
    setStatus(`Deleted ${label}`);
  } catch (e) {
    setStatus(`Delete error: ${e}`);
  }
}

async function extractSelectedPages() {
  if (!currentDoc || selectedPages.size === 0) return;
  const pages = [...selectedPages].sort((a, b) => a - b);
  const base = currentDocPath
    ? currentDocPath.replace(/\\/g, "/").split("/").pop()!.replace(/\.pdf$/i, "")
    : "document";
  const suffix = pages.length === 1
    ? `page_${pages[0] + 1}`
    : `pages_${pages[0] + 1}-${pages[pages.length - 1] + 1}`;
  const defaultName = `${base}_${suffix}.pdf`;
  const path = await save({
    title: "Extract pages to…",
    defaultPath: defaultName,
    filters: [{ name: "PDF", extensions: ["pdf"] }],
  });
  if (!path) return;
  setStatus(`Extracting ${pages.length} page${pages.length === 1 ? "" : "s"}…`);
  try {
    await invoke("extract_pages", { document: currentDoc, pages, path });
    setStatus(`Extracted ${pages.length} page${pages.length === 1 ? "" : "s"} → ${path}`);
  } catch (e) {
    setStatus(`Extract error: ${e}`);
  }
}

async function rotateSelectionOrCurrent(degrees: number) {
  if (!currentDoc) return;
  const pages = selectedPages.size > 0
    ? [...selectedPages].sort((a, b) => a - b)
    : [currentPage];
  const label = pages.length === 1
    ? `page ${pages[0] + 1}`
    : `${pages.length} pages`;
  setStatus(`Rotating ${label}…`);
  try {
    const snap = await invoke<DocSnapshot>("rotate_pages", {
      document: currentDoc,
      pages,
      degrees,
    });
    applyDocSnapshot(snap);
    refreshSelectionUi();
    setStatus(`Rotated ${label} by ${degrees}°`);
  } catch (e) {
    setStatus(`Rotate error: ${e}`);
  }
}

async function deleteCurrentPage() {
  if (!currentDoc) return;
  if (pageCount <= 1) {
    setStatus("Can't delete the only page");
    return;
  }
  const page = currentPage;
  if (!window.confirm(`Delete page ${page + 1}? This can only be undone by not saving.`)) return;
  setStatus(`Deleting page ${page + 1}…`);
  try {
    const snap = await invoke<DocSnapshot>("delete_page", { document: currentDoc, page });
    applyDocSnapshot(snap);
    setStatus(`Deleted page ${page + 1}`);
  } catch (e) {
    setStatus(`Delete error: ${e}`);
  }
}

async function deletePageAt(page: number) {
  if (!currentDoc) return;
  if (pageCount <= 1) {
    setStatus("Can't delete the only page");
    return;
  }
  if (!window.confirm(`Delete page ${page + 1}? This can only be undone by not saving.`)) return;
  setStatus(`Deleting page ${page + 1}…`);
  try {
    const snap = await invoke<DocSnapshot>("delete_page", { document: currentDoc, page });
    applyDocSnapshot(snap);
    setStatus(`Deleted page ${page + 1}`);
  } catch (e) {
    setStatus(`Delete error: ${e}`);
  }
}

async function triggerSaveAs() {
  if (!currentDoc) return;
  const suggestion = currentDocPath
    ? currentDocPath.replace(/\.pdf$/i, "") + "-copy.pdf"
    : "document.pdf";
  const dest = await save({
    defaultPath: suggestion,
    filters: [{ name: "PDF", extensions: ["pdf"] }],
  });
  if (!dest || typeof dest !== "string") return;
  setStatus(`Saving to ${dest}…`);
  try {
    await invoke("save_pdf_as", { document: currentDoc, path: dest });
    setStatus(`Saved: ${dest}`);
  } catch (e) {
    setStatus(`Save error: ${e}`);
  }
}

async function triggerPrint() {
  if (!currentDoc) return;
  setStatus("Opening print dialog…");
  try {
    await invoke("print_document", { document: currentDoc });
    setStatus("Print dialog launched");
  } catch (e) {
    setStatus(`Print error: ${e}`);
  }
}

document.getElementById("saveAsBtn")!.addEventListener("click", triggerSaveAs);

const ribbonBody = document.querySelector(".ribbonBody") as HTMLDivElement;
const modeHint = document.getElementById("modeHint") as HTMLSpanElement;

function setRibbonTab(tab: RibbonTab) {
  ribbonTab = tab;
  ribbonBody.dataset.active = tab;
  document.querySelectorAll(".ribbonTab").forEach((el) => {
    el.classList.toggle("active", (el as HTMLElement).dataset.ribbonTab === tab);
  });
  const wasOrganize = document.body.classList.contains("organize");
  document.body.classList.toggle("organize", tab === "organize");
  if (tab === "organize") {
    // Show sidebar so the full-width thumbs grid is visible.
    document.getElementById("sidebar")!.classList.remove("hidden");
    setEditMode("select");
  } else if (tab === "reader") {
    setEditMode("select");
    clearSelection();
  }
  // Viewer was hidden during organize so fit-mode zoom is stale — recompute.
  if (wasOrganize && tab !== "organize" && currentDoc) {
    requestAnimationFrame(() => reflow());
  }
}

async function setEditMode(mode: EditMode) {
  if (mode === "delete" && editMode !== "delete") {
    const ok = await confirmDialog(
      "When entering Delete mode, no further confirmation is needed to delete pages.\n\nMake sure to use with caution!",
      { title: "Enter Delete mode", kind: "warning", okLabel: "Enter Delete mode", cancelLabel: "Cancel" },
    );
    if (!ok) return;
  }
  editMode = mode;
  document.querySelectorAll(".modeBtn").forEach((el) => {
    const m = (el as HTMLElement).dataset.mode;
    el.classList.toggle("active", m === mode);
    el.classList.toggle("danger", m === mode && mode === "delete");
  });
  document.body.classList.toggle("edit-delete", mode === "delete");
  refreshSelectionUi();
}

document.querySelectorAll(".ribbonTab").forEach((el) => {
  el.addEventListener("click", () => {
    const t = (el as HTMLElement).dataset.ribbonTab as RibbonTab;
    setRibbonTab(t);
  });
});
document.querySelectorAll(".modeBtn").forEach((el) => {
  el.addEventListener("click", () => {
    const m = (el as HTMLElement).dataset.mode as EditMode;
    setEditMode(m);
  });
});
document.getElementById("deleteSelectionBtn")!.addEventListener("click", deleteSelectedPages);
document.getElementById("extractSelectionBtn")!.addEventListener("click", extractSelectedPages);
const rotateMenuBtn = document.getElementById("rotateMenuBtn") as HTMLButtonElement;
const rotateMenu = document.getElementById("rotateMenu") as HTMLDivElement;
function setRotateMenuOpen(open: boolean) {
  rotateMenu.classList.toggle("hidden", !open);
  rotateMenuBtn.setAttribute("aria-expanded", String(open));
}
rotateMenuBtn.addEventListener("click", (e) => {
  e.stopPropagation();
  setRotateMenuOpen(rotateMenu.classList.contains("hidden"));
});
document.addEventListener("click", (e) => {
  if (!rotateMenu.classList.contains("hidden")
      && !rotateMenu.contains(e.target as Node)
      && e.target !== rotateMenuBtn) {
    setRotateMenuOpen(false);
  }
});
function runRotate(deg: number) {
  setRotateMenuOpen(false);
  rotateSelectionOrCurrent(deg);
}
document.getElementById("rotateLeftBtn")!.addEventListener("click", () => runRotate(-90));
document.getElementById("rotateRightBtn")!.addEventListener("click", () => runRotate(90));
document.getElementById("rotate180Btn")!.addEventListener("click", () => runRotate(180));
window.addEventListener("contextmenu", (e) => e.preventDefault());

// Delegated thumb click handler: runs in capture phase before WebView2 can
// apply native image behavior. Reading modifier keys from MouseEvent on
// pointerdown/mousedown proved unreliable on Edge WebView2; pointerup fires
// late enough that modifier state is always populated.
thumbsPanel.addEventListener(
  "pointerdown",
  (e) => {
    const t = (e.target as HTMLElement | null)?.closest(".thumb") as HTMLElement | null;
    if (!t) return;
    const idx = parseInt(t.dataset.page || "-1", 10);
    if (idx < 0) return;
    // In organize mode we rely on a separate click listener (below) so that
    // mousedown can still initiate a native drag without first mutating
    // selection state.
    if (ribbonTab === "organize") return;
    e.preventDefault();
    e.stopPropagation();
    if (ribbonTab === "edit") {
      handleThumbSelect(idx, e);
    } else {
      goToPage(idx);
    }
  },
  true,
);

// Organize-mode pointer-based drag-to-reorder. WebView2's native HTML5 drag
// misfires on div wrappers, so we track pointer events ourselves.
type PDrag = {
  tracking: boolean;
  active: boolean;
  startX: number;
  startY: number;
  startIdx: number;
  pages: number[];
  preview: HTMLElement | null;
  target: HTMLElement | null;
  after: boolean;
};
let pdrag: PDrag = {
  tracking: false, active: false, startX: 0, startY: 0, startIdx: -1,
  pages: [], preview: null, target: null, after: false,
};
let lastWasDrag = false;

function findDropTarget(x: number, y: number): { el: HTMLElement; after: boolean } | null {
  const el = document.elementFromPoint(x, y) as HTMLElement | null;
  const hit = el?.closest(".thumb") as HTMLElement | null;
  if (hit) {
    const r = hit.getBoundingClientRect();
    return { el: hit, after: x > r.left + r.width / 2 };
  }
  // cursor in gap between thumbs — pick nearest by center distance
  let best: HTMLElement | null = null;
  let bestRect: DOMRect | null = null;
  let bestDist = Infinity;
  for (const w of thumbWraps) {
    if (w.classList.contains("dragging")) continue;
    const r = w.getBoundingClientRect();
    const cx = r.left + r.width / 2;
    const cy = r.top + r.height / 2;
    const dx = cx - x;
    const dy = cy - y;
    const d = dx * dx + dy * dy;
    if (d < bestDist) { bestDist = d; best = w; bestRect = r; }
  }
  if (!best || !bestRect) return null;
  return { el: best, after: x > bestRect.left + bestRect.width / 2 };
}

function resetPdrag() {
  if (pdrag.preview) pdrag.preview.remove();
  thumbWraps.forEach((w) => w.classList.remove("dragging"));
  document.querySelectorAll(".thumb.dropBefore, .thumb.dropAfter").forEach((el) => {
    el.classList.remove("dropBefore");
    el.classList.remove("dropAfter");
  });
  pdrag = {
    tracking: false, active: false, startX: 0, startY: 0, startIdx: -1,
    pages: [], preview: null, target: null, after: false,
  };
}

thumbsPanel.addEventListener("pointerdown", (e) => {
  if (ribbonTab !== "organize") return;
  if (e.button !== 0) return;
  const t = (e.target as HTMLElement | null)?.closest(".thumb") as HTMLElement | null;
  if (!t) return;
  const idx = parseInt(t.dataset.page || "-1", 10);
  if (idx < 0) return;
  pdrag = {
    tracking: true, active: false, startX: e.clientX, startY: e.clientY,
    startIdx: idx, pages: [], preview: null, target: null, after: false,
  };
});

window.addEventListener("pointermove", (e) => {
  if (!pdrag.tracking) return;
  if (!pdrag.active) {
    const dx = e.clientX - pdrag.startX;
    const dy = e.clientY - pdrag.startY;
    if (dx * dx + dy * dy < 25) return;
    pdrag.active = true;
    if (selectedPages.has(pdrag.startIdx) && selectedPages.size > 1) {
      pdrag.pages = [...selectedPages].sort((a, b) => a - b);
    } else {
      pdrag.pages = [pdrag.startIdx];
      selectedPages.clear();
      selectedPages.add(pdrag.startIdx);
      selectionAnchor = pdrag.startIdx;
      refreshSelectionUi();
    }
    const pageSet = new Set(pdrag.pages);
    thumbWraps.forEach((w, i) => {
      if (pageSet.has(i)) w.classList.add("dragging");
    });
    const prev = document.createElement("div");
    prev.className = "dragPreview";
    prev.textContent = `Move ${pdrag.pages.length} page${pdrag.pages.length === 1 ? "" : "s"}`;
    document.body.appendChild(prev);
    pdrag.preview = prev;
  }
  if (pdrag.preview) {
    pdrag.preview.style.left = `${e.clientX + 14}px`;
    pdrag.preview.style.top = `${e.clientY + 14}px`;
  }
  const hit = findDropTarget(e.clientX, e.clientY);
  const target = hit?.el ?? null;
  if (pdrag.target && pdrag.target !== target) {
    pdrag.target.classList.remove("dropBefore");
    pdrag.target.classList.remove("dropAfter");
  }
  pdrag.target = target;
  if (hit && target) {
    pdrag.after = hit.after;
    target.classList.toggle("dropBefore", !pdrag.after);
    target.classList.toggle("dropAfter", pdrag.after);
  }
});

window.addEventListener("pointerup", async () => {
  if (!pdrag.tracking) return;
  const wasActive = pdrag.active;
  const pages = pdrag.pages;
  const target = pdrag.target;
  const after = pdrag.after;
  resetPdrag();
  if (!wasActive) return;
  lastWasDrag = true;
  setTimeout(() => { lastWasDrag = false; }, 0);
  if (pages.length === 0) return;
  const dragSet = new Set(pages);
  const remaining = [...Array(pageCount).keys()].filter((i) => !dragSet.has(i));
  let insertAt: number;
  if (!target) {
    insertAt = remaining.length;
  } else {
    const tIdx = parseInt(target.dataset.page || "-1", 10);
    if (dragSet.has(tIdx)) {
      insertAt = remaining.length;
    } else {
      insertAt = remaining.indexOf(tIdx);
      if (after) insertAt += 1;
    }
  }
  const newOrder = [
    ...remaining.slice(0, insertAt),
    ...pages,
    ...remaining.slice(insertAt),
  ];
  if (newOrder.length !== pageCount) return;
  if (newOrder.every((v, i) => v === i)) return;
  setStatus(`Reordering ${pages.length} page${pages.length === 1 ? "" : "s"}…`);
  try {
    const snap = await invoke<DocSnapshot>("reorder_pages", { document: currentDoc, order: newOrder });
    clearSelection();
    applyDocSnapshot(snap);
    setStatus(`Reordered ${pages.length} page${pages.length === 1 ? "" : "s"}`);
  } catch (err) {
    setStatus(`Reorder error: ${err}`);
  }
});

window.addEventListener("pointercancel", () => resetPdrag());

// Organize-mode selection via click — suppressed if a drag just completed.
thumbsPanel.addEventListener("click", (e) => {
  if (ribbonTab !== "organize") return;
  if (lastWasDrag) return;
  const t = (e.target as HTMLElement | null)?.closest(".thumb") as HTMLElement | null;
  if (!t) return;
  const idx = parseInt(t.dataset.page || "-1", 10);
  if (idx < 0) return;
  handleThumbSelect(idx, e);
});


document.getElementById("printBtn")!.addEventListener("click", triggerPrint);
document.getElementById("propsBtn")!.addEventListener("click", openProperties);
document.getElementById("propsClose")!.addEventListener("click", closeProperties);
propsOverlay.addEventListener("click", (e) => {
  if (e.target === propsOverlay) closeProperties();
});
document.querySelectorAll(".propsTab").forEach((el) => {
  el.addEventListener("click", () => {
    document.querySelectorAll(".propsTab").forEach((t) => t.classList.remove("active"));
    el.classList.add("active");
    const ptab = (el as HTMLElement).dataset.ptab;
    propsInfoPanel.classList.toggle("hidden", ptab !== "info");
    propsPermsPanel.classList.toggle("hidden", ptab !== "perms");
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
  renderAllHighlights();
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
    renderAllHighlights();
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
    goToPage(target);
  } else {
    renderAllHighlights();
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

function openGoto() {
  if (!currentDoc) return;
  gotoPanel.classList.remove("hidden");
  gotoInput.value = String(currentPage + 1);
  gotoInput.max = String(pageCount);
  gotoInput.min = "1";
  gotoInput.focus();
  gotoInput.select();
}
function closeGoto() {
  gotoPanel.classList.add("hidden");
}
function submitGoto() {
  const v = parseInt(gotoInput.value, 10);
  if (!isNaN(v)) goToPage(v - 1);
  closeGoto();
}
document.getElementById("gotoBtn")!.addEventListener("click", openGoto);
document.getElementById("gotoGo")!.addEventListener("click", submitGoto);
document.getElementById("gotoClose")!.addEventListener("click", closeGoto);
gotoInput.addEventListener("keydown", (e) => {
  if (e.key === "Enter") {
    e.preventDefault();
    submitGoto();
  } else if (e.key === "Escape") {
    closeGoto();
  }
});

async function toggleFullscreen() {
  try {
    const win = getCurrentWindow();
    const full = await win.isFullscreen();
    await win.setFullscreen(!full);
  } catch (e) {
    setStatus(`Fullscreen error: ${e}`);
  }
}

let presentationActive = false;
async function togglePresentation() {
  presentationActive = !presentationActive;
  document.body.classList.toggle("presentation", presentationActive);
  try {
    await getCurrentWindow().setFullscreen(presentationActive);
  } catch {}
  if (presentationActive) {
    viewMode = "single";
    viewModeSelect.value = "single";
    fitMode = "page";
    fitSelect.value = "page";
    applyViewMode();
    reflow();
  }
}

document.getElementById("fullscreenBtn")!.addEventListener("click", toggleFullscreen);
document.getElementById("presentBtn")!.addEventListener("click", togglePresentation);

window.addEventListener("keydown", (e) => {
  const inField = document.activeElement && /INPUT|TEXTAREA/.test(document.activeElement.tagName);
  if (e.key === "F11") {
    e.preventDefault();
    toggleFullscreen();
    return;
  }
  if (e.key === "F5") {
    e.preventDefault();
    togglePresentation();
    return;
  }
  if (e.key === "F4") {
    e.preventDefault();
    toggleSidebar();
    return;
  }
  if (e.altKey && e.key === "ArrowLeft") {
    e.preventDefault();
    historyBack();
    return;
  }
  if (e.altKey && e.key === "ArrowRight") {
    e.preventDefault();
    historyForward();
    return;
  }
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
  if (e.ctrlKey && (e.key === "g" || e.key === "G")) {
    e.preventDefault();
    openGoto();
    return;
  }
  if (e.ctrlKey && (e.key === "p" || e.key === "P")) {
    e.preventDefault();
    triggerPrint();
    return;
  }
  if (e.ctrlKey && e.shiftKey && (e.key === "s" || e.key === "S")) {
    e.preventDefault();
    triggerSaveAs();
    return;
  }
  if (!inField && ribbonTab === "edit" && (e.key === "Delete" || e.key === "Del")) {
    e.preventDefault();
    if (selectedPages.size > 0) deleteSelectedPages();
    return;
  }
  if (e.key === "F3") {
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
    reflow();
    return;
  }
  if (e.ctrlKey && e.key === "1") {
    e.preventDefault();
    fitMode = "page";
    fitSelect.value = "page";
    reflow();
    return;
  }
  if (e.ctrlKey && e.key === "2") {
    e.preventDefault();
    fitMode = "width";
    fitSelect.value = "width";
    reflow();
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
    if (!propsOverlay.classList.contains("hidden")) closeProperties();
    else if (!searchPanel.classList.contains("hidden")) closeSearch();
    else if (!gotoPanel.classList.contains("hidden")) closeGoto();
    else if (presentationActive) togglePresentation();
  }
});

window.addEventListener("resize", () => {
  if (fitMode === "width" || fitMode === "page") reflow();
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
