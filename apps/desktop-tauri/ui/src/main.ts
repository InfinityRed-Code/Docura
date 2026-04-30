import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
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
      <button class="ribbonTab" data-ribbon-tab="convert">Convert</button>
      <button class="ribbonTab" data-ribbon-tab="organize">Organize</button>
      <button id="settingsBtnTop" class="ribbonTab settingsTab" title="Settings">⚙</button>
    </div>
    <div class="ribbonBody" data-active="reader">
      <button data-ribbon="both" id="sidebarBtn" title="Toggle sidebar (F4)">☰</button>
      <button data-ribbon="both" id="commentToggleBtn" title="Toggle comment panel (F7)">💬</button>
      <button data-ribbon="both" id="open" title="Open (Ctrl+O)">📂</button>
      <button data-ribbon="both" id="recentBtn" title="Recent files">🕘</button>
      <div id="recentMenu" class="menu hidden"></div>
      <span data-ribbon="both" class="sep"></span>
      <button data-ribbon="both" id="undoBtn" title="Undo (Ctrl+Z)" disabled>↶</button>
      <button data-ribbon="both" id="redoBtn" title="Redo (Ctrl+Y)" disabled>↷</button>
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
      <button data-ribbon="reader" id="wheelZoomBtn" title="Zoom mode — scroll the mouse wheel to zoom around the cursor; left-drag to pan">🔎</button>
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
      <button data-ribbon="reader" id="searchBtn" title="Search (Ctrl+F)">🔍</button>
      <button data-ribbon="both" id="saveAsBtn" title="Save As (Ctrl+Shift+S)">💾</button>
      <button data-ribbon="reader" id="printBtn" title="Print (Ctrl+P)">🖨</button>
      <button data-ribbon="reader" id="propsBtn" title="Document properties">ⓘ</button>
      <div data-ribbon="edit" class="ribbonGroup">
        <div class="groupBody">
          <div class="modeGroup" role="radiogroup">
            <button class="modeBtn active" data-mode="select" title="Select (no page edits)">👆</button>
            <button class="modeBtn" data-mode="delete" title="Delete mode — select pages with click / Ctrl+click / Shift+click, then press the trash button">🗑</button>
          </div>
          <button id="deleteSelectionBtn" class="deleteAction" disabled title="Delete selected pages">🗑</button>
          <button id="extractSelectionBtn" disabled title="Save selected pages as a new PDF">⬇</button>
        </div>
        <div class="groupLabel">Modi</div>
      </div>
      <div data-ribbon="edit" class="ribbonGroup">
        <div class="groupBody">
          <div class="menuHost">
            <button id="rotateMenuBtn" class="menuTrigger" aria-haspopup="true" aria-expanded="false" title="Rotate pages">⟳</button>
            <div id="rotateMenu" class="popover hidden" role="menu">
              <button id="rotateLeftBtn" role="menuitem" title="Rotate 90° counter-clockwise">↺</button>
              <button id="rotateRightBtn" role="menuitem" title="Rotate 90° clockwise">↻</button>
              <button id="rotate180Btn" role="menuitem" title="Rotate 180°">⇅</button>
            </div>
          </div>
        </div>
        <div class="groupLabel">Rotate</div>
      </div>
      <div data-ribbon="edit" class="ribbonGroup">
        <div class="groupBody">
          <div class="menuHost">
            <button id="insertMenuBtn" class="menuTrigger" aria-haspopup="true" aria-expanded="false" title="Insert pages from another PDF">＋</button>
            <div id="insertMenu" class="popover hidden" role="menu">
              <button id="insertBeforeBtn" role="menuitem" title="Insert selected PDFs before current page">↥</button>
              <button id="insertAfterBtn" role="menuitem" title="Insert selected PDFs after current page">↧</button>
              <button id="insertEndBtn" role="menuitem" title="Append pages to the end of the document">⤓</button>
            </div>
          </div>
        </div>
        <div class="groupLabel">Insert</div>
      </div>
      <span data-ribbon="edit" id="modeHint" class="modeHint"></span>
      <button data-ribbon="organize" id="mergePdfsBtn" title="Merge PDFs: pick files and append them">⊕</button>
      <button data-ribbon="organize" id="splitPdfBtn" title="Split into multiple PDFs by page ranges">✂</button>
      <div data-ribbon="edit" class="ribbonGroup">
        <div class="groupBody">
          <div class="modeGroup" role="radiogroup" id="annotToolGroup" title="Mark text — drag to select, release to apply">
            <button class="modeBtn annotTool" data-annot="highlight" title="Highlight selected text">🖍</button>
            <button class="modeBtn annotTool" data-annot="underline" title="Underline selected text"><u>U</u></button>
            <button class="modeBtn annotTool" data-annot="strikeout" title="Strike through selected text">S̶</button>
          </div>
          <input type="color" id="annotColor" value="#ffe600" title="Annotation color" style="height:1.6rem;width:1.8rem;padding:0;border:1px solid #4a4a4a;background:#3a3a3a;cursor:pointer" />
          <button class="modeBtn active" id="printToggleBtn" title="Include new annotations when printing (toggle)">🖨</button>
        </div>
        <div class="groupLabel">Text Decoration</div>
      </div>
      <div data-ribbon="edit" class="ribbonGroup">
        <div class="groupBody">
          <div class="modeGroup" role="radiogroup" id="shapeToolGroup" title="Drag on a page to draw">
            <button class="modeBtn shapeTool" data-shape="rect" title="Rectangle">▭</button>
            <button class="modeBtn shapeTool" data-shape="circle" title="Ellipse">◯</button>
            <button class="modeBtn shapeTool" data-shape="line" title="Line">／</button>
            <button class="modeBtn shapeTool" data-shape="arrow" title="Arrow">↗</button>
            <button class="modeBtn shapeTool" data-shape="freetext" title="Text box: drag a rectangle, then type">T</button>
            <button class="modeBtn shapeTool" data-shape="ink" title="Free-hand pen">✎</button>
            <button class="modeBtn shapeTool" data-shape="stamp" title="Stamp an image: pick a file, then drag a rectangle on the page">🔖</button>
            <button class="modeBtn shapeTool" data-shape="note" title="Sticky note: click on the page, then type a comment">📝</button>
          </div>
        </div>
        <div class="groupLabel">Draw</div>
      </div>
      <div data-ribbon="edit" class="ribbonGroup">
        <div class="groupBody">
          <div class="modeGroup" role="radiogroup">
            <button class="modeBtn shapeTool" data-shape="region" title="Save region as new PDF: drag a rectangle on the page, choose where to save">✂</button>
          </div>
        </div>
        <div class="groupLabel">Region</div>
      </div>
      <div data-ribbon="edit" class="ribbonGroup">
        <div class="groupBody">
          <button id="watermarkBtn" title="Watermark — stamp text or image on every page">💧</button>
          <button id="headerFooterBtn" title="Header / Footer / page numbers">🅷</button>
        </div>
        <div class="groupLabel">Layout</div>
      </div>
      <span data-ribbon="organize" id="organizeHint" class="modeHint">Drag thumbnails to reorder. Click / Ctrl+click / Shift+click to select multiple.</span>
      <div data-ribbon="convert" class="ribbonGroup">
        <div class="groupBody">
          <div class="modeGroup" role="radiogroup">
            <button class="modeBtn active" data-conv="pdfa" title="Convert PDF files to PDF/A" type="button">📄→Ⓐ</button>
          </div>
        </div>
        <div class="groupLabel">Conversion</div>
      </div>
      <span data-ribbon="convert" class="modeHint">Pick PDF files in the panel below, choose a level + output folder, then ✔ to convert.</span>
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
    <aside id="commentPanel" class="commentPanel hidden">
      <div class="commentHeader">
        <strong>Comments</strong>
        <button id="commentReloadBtn" title="Refresh">↻</button>
        <button id="commentExportBtn" title="Export annotations as XFDF">⤓</button>
        <button id="commentImportBtn" title="Import annotations from XFDF">⤒</button>
        <button id="commentCloseBtn" class="propsClose" title="Close">✕</button>
      </div>
      <div class="commentFilters">
        <select id="commentKindFilter">
          <option value="">All kinds</option>
          <option value="Highlight">Highlight</option>
          <option value="Underline">Underline</option>
          <option value="StrikeOut">Strikeout</option>
          <option value="Square">Rectangle</option>
          <option value="Circle">Circle</option>
          <option value="Line">Line</option>
          <option value="Ink">Ink</option>
          <option value="FreeText">Text box</option>
          <option value="Stamp">Stamp</option>
          <option value="Text">Sticky</option>
        </select>
        <input id="commentAuthorFilter" placeholder="Author…" />
        <label class="commentChk"><input type="checkbox" id="commentHasContent" /> with comment</label>
      </div>
      <div id="commentList" class="commentList"></div>
    </aside>
    <div id="mergePane" class="mergePane hidden">
      <div class="mergeHeader">
        <strong>Merge PDFs</strong>
        <button id="mergeAddBtn" title="Pick PDF files">＋</button>
        <button id="mergeCommitBtn" disabled title="Build merged PDF and open it">✔</button>
        <button id="mergeCancelBtn" title="Cancel">✕</button>
        <span class="mergeHint">Drop PDFs here or click Add. Drag tiles to reorder. Click ✔ to build.</span>
      </div>
      <div id="mergeList" class="mergeList"></div>
    </div>
    <div id="convertPane" class="mergePane convertPane hidden">
      <div class="mergeHeader">
        <strong>Convert PDF → PDF/A</strong>
        <button id="convertAddBtn" title="Pick PDF files">＋</button>
        <select id="convertLevel" title="PDF/A level">
          <option value="1b">PDF/A-1b</option>
          <option value="2b" selected>PDF/A-2b</option>
          <option value="3b">PDF/A-3b</option>
          <option value="4">PDF/A-4</option>
        </select>
        <button id="convertOutBtn" title="Pick output folder">📂</button>
        <button id="convertCommitBtn" disabled title="Convert all files">✔</button>
        <button id="convertCancelBtn" title="Cancel">✕</button>
        <span id="convertOutLabel" class="mergeHint">Output: (none chosen)</span>
      </div>
      <div id="convertList" class="mergeList"></div>
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
  <div id="settingsOverlay" class="propsOverlay hidden">
    <div class="propsModal">
      <div class="propsHeader"><strong>Settings</strong><button id="settingsClose" class="propsClose">✕</button></div>
      <div class="propsPanel wmPanel">
        <h3 style="margin:0 0 0.3rem 0;font-size:0.95rem;">PDF/A export</h3>
        <div id="gsStatus" style="font-size:0.85rem;color:#bbb;">Checking Ghostscript…</div>
        <div id="gsInstall" class="hidden" style="font-size:0.85rem;">
          Not found. Install from
          <a id="gsLink" href="https://ghostscript.com/releases/gsdnld.html" style="color:#4a8cff;">ghostscript.com</a>.
        </div>
        <label>Default PDF/A level
          <select id="pdfaLevel">
            <option value="off" selected>Off (plain PDF)</option>
            <option value="1b">PDF/A-1b (ISO 19005-1)</option>
            <option value="2b">PDF/A-2b (ISO 19005-2)</option>
            <option value="3b">PDF/A-3b (ISO 19005-3)</option>
            <option value="4">PDF/A-4 (ISO 19005-4)</option>
          </select>
        </label>
        <div style="font-size:0.78rem;color:#999;">When set to a level, all Save As exports and virtual-printer output are converted to that PDF/A level via Ghostscript.</div>
        <div class="wmActions"><button id="pdfaSaveBtn">Save current PDF as PDF/A now…</button><button id="gsRecheckBtn">Re-check Ghostscript</button></div>
      </div>
      <div class="propsPanel wmPanel">
        <h3 style="margin:0 0 0.3rem 0;font-size:0.95rem;">Virtual printer (Print to PDF)</h3>
        <div id="vpStatus" style="font-size:0.85rem;color:#bbb;">Checking virtual printer…</div>
        <div id="vpPathRow" class="hidden" style="font-size:0.8rem;color:#999;"></div>
        <div id="vpOutRow" style="font-size:0.8rem;color:#999;"></div>
        <div id="vpWarn" class="hidden" style="font-size:0.8rem;color:#d98a8a;">Ghostscript is required to convert print jobs.</div>
        <div class="wmActions">
          <button id="vpInstallBtn">Install printer</button>
          <button id="vpUninstallBtn">Uninstall printer</button>
          <button id="vpOpenFolderBtn">Open output folder</button>
        </div>
        <div style="font-size:0.75rem;color:#888;">Printing from any app to <strong>rustpdf</strong> saves a PDF to the output folder. Needs admin once to install.</div>
      </div>
    </div>
  </div>
  <div id="wmOverlay" class="propsOverlay hidden">
    <div class="propsModal">
      <div class="propsHeader"><strong>Watermark</strong><button id="wmClose" class="propsClose">✕</button></div>
      <div class="ribbonTabs" role="tablist">
        <button id="wmTabText" class="ribbonTab active" data-wm-tab="text">Text</button>
        <button id="wmTabImage" class="ribbonTab" data-wm-tab="image">Image</button>
      </div>
      <div id="wmSectionText" class="propsPanel wmPanel">
        <label>Text<input id="wmText" type="text" placeholder="DRAFT" /></label>
        <label>Font size<input id="wmFontSize" type="number" min="8" max="400" step="1" value="72" /></label>
        <label>Rotation (°)<input id="wmRot" type="number" min="-180" max="180" step="1" value="45" /></label>
        <label>Opacity<input id="wmOpacity" type="number" min="0.05" max="1" step="0.05" value="0.25" /></label>
        <label>Gray (0=black,1=white)<input id="wmGray" type="number" min="0" max="1" step="0.05" value="0.5" /></label>
      </div>
      <div id="wmSectionImage" class="propsPanel wmPanel hidden">
        <label>Image file<span style="display:flex;gap:0.3rem;align-items:center;"><input id="iwPath" type="text" readonly placeholder="Pick an image…" style="flex:1;" /><button id="iwPick">Browse…</button></span></label>
        <label>Scale (fraction of page width)<input id="iwScale" type="number" min="0.05" max="1.5" step="0.05" value="0.4" /></label>
        <label>Rotation (°)<input id="iwRot" type="number" min="-180" max="180" step="1" value="0" /></label>
        <label>Opacity<input id="iwOpacity" type="number" min="0.05" max="1" step="0.05" value="0.5" /></label>
        <label>Anchor
          <select id="iwAnchor">
            <option value="center" selected>Center</option>
            <option value="topleft">Top-left</option>
            <option value="topright">Top-right</option>
            <option value="bottomleft">Bottom-left</option>
            <option value="bottomright">Bottom-right</option>
          </select>
        </label>
        <label>Margin (pt)<input id="iwMargin" type="number" min="0" max="400" step="1" value="36" /></label>
      </div>
      <div class="wmActions" style="padding: 0.5rem 0.8rem;"><button id="wmCommit">Apply</button><button id="wmCancel">Cancel</button></div>
    </div>
  </div>
  <div id="hfOverlay" class="propsOverlay hidden">
    <div class="propsModal">
      <div class="propsHeader"><strong>Header / Footer / Page numbers</strong><button id="hfClose" class="propsClose">✕</button></div>
      <div class="propsPanel wmPanel">
        <div class="hfHint">Tokens: <code>{page}</code>, <code>{total}</code></div>
        <div class="hfGrid">
          <label>Header left<input id="hfHL" type="text" /></label>
          <label>Header center<input id="hfHC" type="text" /></label>
          <label>Header right<input id="hfHR" type="text" /></label>
          <label>Footer left<input id="hfFL" type="text" /></label>
          <label>Footer center<input id="hfFC" type="text" placeholder="Page {page} of {total}" /></label>
          <label>Footer right<input id="hfFR" type="text" /></label>
        </div>
        <div class="hfRow">
          <label>Font size<input id="hfFontSize" type="number" min="6" max="48" step="1" value="10" /></label>
          <label>Margin (pt)<input id="hfMargin" type="number" min="0" max="200" step="1" value="24" /></label>
          <label>First page #<input id="hfFirst" type="number" min="1" step="1" value="1" /></label>
          <label>Gray<input id="hfGray" type="number" min="0" max="1" step="0.05" value="0" /></label>
        </div>
        <div class="wmActions"><button id="hfCommit">Apply</button><button id="hfCancel">Cancel</button></div>
      </div>
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
  .ribbonTabs { display:flex; gap:0; border-bottom:1px solid #2f2f2f; padding: 0 0.4rem; align-items:center; }
  .ribbonTab.settingsTab { margin-left:auto; padding:0.2rem 0.6rem; font-size:1rem;
                           color:#bbb; cursor:pointer; }
  .ribbonTab.settingsTab:hover { color:#fff; background:#333; }
  .ribbonTab.settingsTab.active { border-bottom:none; color:#fff; }
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
  .ribbonBody button.active { background:#4a8cff; color:#fff; border-color:#4a8cff; }
  body.wheelZoomMode .viewer { cursor: grab; }
  body.wheelZoomMode.panning .viewer { cursor: grabbing; }
  body.wheelZoomMode .selectLayer, body.wheelZoomMode .drawLayer { pointer-events: none; }
  body.wheelZoomMode .pageSlot, body.wheelZoomMode .pageSlot * { user-select: none; -webkit-user-select: none; }
  .ribbonBody[data-active="reader"] > [data-ribbon="edit"],
  .ribbonBody[data-active="reader"] > [data-ribbon="organize"],
  .ribbonBody[data-active="reader"] > [data-ribbon="convert"] { display:none; }
  .ribbonBody[data-active="edit"] > [data-ribbon="reader"],
  .ribbonBody[data-active="edit"] > [data-ribbon="organize"],
  .ribbonBody[data-active="edit"] > [data-ribbon="convert"] { display:none; }
  .ribbonBody[data-active="organize"] > [data-ribbon="reader"],
  .ribbonBody[data-active="organize"] > [data-ribbon="edit"],
  .ribbonBody[data-active="organize"] > [data-ribbon="convert"] { display:none; }
  /* Convert tab is its own page — hide everything except convert-specific groups. */
  .ribbonBody[data-active="convert"] > [data-ribbon="reader"],
  .ribbonBody[data-active="convert"] > [data-ribbon="edit"],
  .ribbonBody[data-active="convert"] > [data-ribbon="organize"],
  .ribbonBody[data-active="convert"] > [data-ribbon="both"] { display:none; }
  .ribbonLabel { color:#888; font-size:0.85rem; margin-left:0.2rem; }
  .ribbonGroup { display:inline-flex; flex-direction:column; align-items:stretch; align-self:stretch;
                 padding: 0 0.5rem; border-right:1px solid #3a3a3a; gap: 0.15rem; }
  .ribbonGroup:last-child { border-right:none; }
  .ribbonGroup .groupBody { display:flex; align-items:center; gap:0.25rem; flex:1; }
  .ribbonGroup .groupLabel { text-align:center; font-size:0.65rem; color:#888;
                             text-transform:uppercase; letter-spacing:0.05em; padding-top:0.1rem; }
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
  .commentPanel { width: 300px; background:#252526; border-left:1px solid #333; display:flex; flex-direction:column; min-height:0; }
  .commentPanel.hidden { display:none; }
  .commentHeader { display:flex; gap:0.3rem; align-items:center; padding:0.4rem 0.5rem; border-bottom:1px solid #333; }
  .commentHeader strong { flex:1; }
  .commentHeader button { background:#3a3a3a; color:#ddd; border:1px solid #4a4a4a; border-radius:3px; padding:0.15rem 0.4rem; cursor:pointer; font-size:0.85rem; }
  .commentHeader button:hover { background:#4a4a4a; }
  .commentFilters { display:flex; flex-direction:column; gap:0.25rem; padding:0.35rem 0.5rem; border-bottom:1px solid #333; font-size:0.8rem; }
  .commentFilters select, .commentFilters input[type=text] { background:#3a3a3a; color:#ddd; border:1px solid #555; padding:0.15rem 0.3rem; font-size:0.8rem; }
  .commentChk { display:flex; align-items:center; gap:0.25rem; color:#bbb; }
  .commentList { flex:1; overflow:auto; padding:0.3rem; }
  .commentGroup { margin-bottom:0.5rem; }
  .commentGroupHead { font-size:0.78rem; color:#888; padding:0.2rem 0.3rem; text-transform:uppercase; letter-spacing:0.5px; }
  .commentItem { padding:0.4rem 0.5rem; border:1px solid #333; border-radius:3px; margin-bottom:0.3rem; cursor:pointer; background:#2a2a2b; }
  .commentItem:hover { background:#333; border-color:#4a4a4a; }
  .commentItem .row1 { display:flex; gap:0.3rem; align-items:center; font-size:0.78rem; color:#aaa; }
  .commentItem .row1 .kind { font-weight:bold; color:#ddd; }
  .commentItem .row1 .author { color:#88c; }
  .commentItem .row1 .date { margin-left:auto; color:#777; font-size:0.72rem; }
  .commentItem .body { font-size:0.85rem; color:#ddd; margin-top:0.2rem; white-space:pre-wrap; word-break:break-word; }
  .commentItem .nopaint { color:#c08; font-size:0.7rem; }
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
  body.wheelZoomMode .viewer { padding:0; }
  .pageList { display:flex; flex-direction:column; align-items:center; gap: 1rem; box-sizing:border-box; }
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
  .drawLayer { position:absolute; inset:0; pointer-events:none; overflow:visible; }
  body.shapeMode .drawLayer { pointer-events:auto; cursor:crosshair; }
  body.shapeMode .selectLayer { pointer-events:none; }
  .highlightLayer .hit { position:absolute; background: rgba(255,220,0,0.45); }
  .highlightLayer .hit.current { background: rgba(255,140,0,0.6); outline:1px solid orange; }
  .linkLayer { position:absolute; inset:0; pointer-events:none; }
  .linkLayer a { position:absolute; pointer-events:auto; display:block;
                 background: transparent; border: 0; cursor:pointer;
                 text-decoration:none; }
  .linkLayer a:hover { background: rgba(74,140,255,0.22); outline: 1px solid rgba(74,140,255,0.6); }
  .formLayer { position:absolute; inset:0; pointer-events:none; }
  .formLayer .formField { position:absolute; pointer-events:auto;
                  background: rgba(74,140,255,0.10); border: 1px solid rgba(74,140,255,0.45);
                  box-sizing:border-box; }
  .formLayer .formField:hover { background: rgba(74,140,255,0.18); }
  .formLayer input[type=text], .formLayer textarea, .formLayer select {
                  width:100%; height:100%; box-sizing:border-box; padding:0 2px;
                  border:0; background:transparent; color:#000; font: inherit;
                  font-family: Helvetica, Arial, sans-serif; resize:none; }
  .formLayer input[type=text]:focus, .formLayer textarea:focus, .formLayer select:focus {
                  background: rgba(255,255,255,0.85); outline:1px solid #4a8cff; }
  .formLayer .formCheck { display:flex; align-items:center; justify-content:center; cursor:pointer; }
  .formLayer .formCheck input { margin:0; transform: scale(1.2); }
  .formLayer .formField.readonly { background: rgba(150,150,150,0.10); border-color: rgba(150,150,150,0.4); }
  .formLayer .formField.required { border-color: rgba(220,160,40,0.7); }
  .formLayer .formField.signature { background: rgba(160,80,160,0.08); border:1px dashed rgba(160,80,160,0.55); display:flex; align-items:center; justify-content:center; color:#a05ca0; font-size:0.78rem; pointer-events:none; }
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
  .mergePane { flex:1; display:none; flex-direction:column; background:#1e1e1e; overflow:hidden; }
  body.merge #mergePane { display:flex; }
  body.convert #convertPane { display:flex; }
  body.convert .viewer { display:none; }
  body.merge #viewer, body.merge #sidebar { display:none !important; }
  .mergeHeader {
    display:flex; align-items:center; gap:0.6rem;
    padding:0.6rem 1rem; border-bottom:1px solid #333; background:#252526;
  }
  .mergeHeader button {
    background:#3a3a3a; color:#ddd; border:1px solid #4a4a4a; border-radius:4px;
    padding:0.35rem 0.7rem; cursor:pointer; font:inherit;
  }
  .mergeHeader button:hover:not(:disabled) { background:#4a4a4a; }
  .mergeHeader button:disabled { opacity:0.4; cursor:not-allowed; }
  #mergeCommitBtn:not(:disabled) { background:#2d5d2d; border-color:#3d7d3d; }
  #mergeCommitBtn:not(:disabled):hover { background:#3d7d3d; }
  .mergeHint { color:#888; font-size:0.85rem; margin-left:auto; }
  .mergeList {
    flex:1; overflow:auto; padding:1rem;
    display:grid; grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap:1rem; align-content:start;
  }
  body.merge.mergeDrop .mergeList { box-shadow: inset 0 0 0 3px #4a8cff; }
  .mergeTile {
    position:relative; background:#2d2d30; border:2px solid #3a3a3a; border-radius:6px;
    padding:0.8rem; display:flex; flex-direction:column; gap:0.3rem;
    cursor:grab; user-select:none;
  }
  .mergeTile:active { cursor:grabbing; }
  .mergeTile .mergeIndex {
    position:absolute; top:4px; left:6px;
    background:rgba(74,140,255,0.9); color:#fff; font-size:0.75rem;
    padding:1px 6px; border-radius:10px; font-variant-numeric:tabular-nums;
  }
  .mergeTile .mergeName {
    font-size:0.9rem; color:#ddd; word-break:break-all;
    margin-top:1.2rem; overflow:hidden; display:-webkit-box;
    -webkit-line-clamp:3; -webkit-box-orient:vertical;
  }
  .mergeTile .mergePath { font-size:0.72rem; color:#777; word-break:break-all; }
  .mergeTile .mergeRemove {
    position:absolute; top:4px; right:6px; background:transparent; color:#888;
    border:none; cursor:pointer; font-size:1rem; padding:0 4px;
  }
  .mergeTile .mergeRemove:hover { color:#e06060; }
  .mergeTile.dragging { opacity:0.35; }
  .mergeTile.dropBefore { box-shadow: inset 4px 0 0 #4a8cff; }
  .mergeTile.dropAfter  { box-shadow: inset -4px 0 0 #4a8cff; }
  .mergeEmpty { color:#777; padding:2rem; text-align:center; grid-column: 1 / -1; font-size:0.95rem; }
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
  .wmPanel label { display:flex; flex-direction:column; gap:0.2rem; font-size:0.85rem; color:#aaa; margin:0.35rem 0; }
  .wmPanel input[type="text"], .wmPanel input[type="number"] { background:#1e1e1e; color:#ddd; border:1px solid #444; border-radius:3px; padding:0.3rem 0.5rem; font:inherit; }
  .wmActions { display:flex; gap:0.5rem; justify-content:flex-end; margin-top:0.6rem; }
  .wmActions button { background:#3a3a3a; color:#ddd; border:1px solid #4a4a4a; border-radius:3px; padding:0.3rem 0.8rem; cursor:pointer; }
  .wmActions button:hover { background:#4a4a4a; }
  #wmCommit, #hfCommit { background:#4a8cff; border-color:#4a8cff; color:#fff; }
  #wmCommit:hover, #hfCommit:hover { background:#5a9cff; }
  .hfHint { color:#888; font-size:0.82rem; margin-bottom:0.4rem; }
  .hfHint code { background:#1e1e1e; padding:0 0.25rem; border-radius:2px; color:#ccc; }
  .hfGrid { display:grid; grid-template-columns:1fr 1fr 1fr; gap:0.4rem; }
  .hfRow { display:grid; grid-template-columns:1fr 1fr 1fr 1fr; gap:0.4rem; margin-top:0.4rem; }
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
  drawLayer: SVGSVGElement;
  formLayer: HTMLDivElement;
  placeholder: HTMLDivElement;
  loaded: boolean;
  renderingZoom: number | null;
  lastZoom: number | null;
};

let currentDoc: string | null = null;
let currentDocPath: string | null = null;
type RibbonTab = "reader" | "edit" | "convert" | "organize";
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

function setStatus(s: string) {
  statusEl.textContent = s;
}

function updateChrome() {
  if (!currentDoc) {
    pageInput.value = "1";
    pageTotal.textContent = "/ 0";
    syncZoomUi();
    return;
  }
  pageInput.value = String(currentPage + 1);
  pageInput.max = String(pageCount);
  pageTotal.textContent = `/ ${pageCount}`;
  syncZoomUi();
  thumbWraps.forEach((w, i) => w.classList.toggle("active", i === currentPage));
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
  const drawLayer = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  drawLayer.setAttribute("class", "drawLayer");
  drawLayer.dataset.page = String(pageIdx);
  const formLayer = document.createElement("div");
  formLayer.className = "formLayer";
  formLayer.dataset.page = String(pageIdx);
  const placeholder = document.createElement("div");
  placeholder.className = "placeholder";
  placeholder.textContent = `${pageIdx + 1}`;
  wrap.appendChild(placeholder);
  wrap.appendChild(img);
  wrap.appendChild(selectLayer);
  wrap.appendChild(highlightLayer);
  wrap.appendChild(linkLayer);
  wrap.appendChild(drawLayer);
  wrap.appendChild(formLayer);
  return {
    wrap,
    img,
    selectLayer,
    highlightLayer,
    linkLayer,
    drawLayer,
    formLayer,
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
  recenterScrollX();
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
  // Buffer the pageList only in zoom mode so the cursor-anchored zoom always
  // has scroll room — including when the page is smaller than the viewer.
  // Classic mode keeps the page top-aligned in the viewer.
  if (wheelZoomMode) {
    const bx = Math.max(16, viewer.clientWidth);
    const by = Math.max(16, viewer.clientHeight);
    pageList.style.padding = `${by}px ${bx}px`;
  } else {
    pageList.style.padding = "";
  }
}

function recenterScrollX() {
  const extra = viewer.scrollWidth - viewer.clientWidth;
  if (extra > 0) viewer.scrollLeft = extra / 2;
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
  recenterScrollX();
  if (viewMode === "single") {
    applyViewMode();
    scheduleRender(currentPage);
    const slot = slots[currentPage];
    if (slot) slot.wrap.scrollIntoView({ block: "start", inline: "center" });
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

function navigateTo(idx: number, _opts: { recordHistory?: boolean } = {}) {
  const clamped = Math.max(0, Math.min(pageCount - 1, idx));
  if (clamped === currentPage) {
    updateChrome();
    return;
  }
  currentPage = clamped;
  if (viewMode === "single") {
    applyViewMode();
    scheduleRender(clamped);
  } else {
    const slot = slots[clamped];
    if (slot) slot.wrap.scrollIntoView({ block: "start", inline: "center", behavior: "auto" });
    scheduleRender(clamped);
  }
  evictFarSlots();
  updateChrome();
}

function goToPage(idx: number) {
  navigateTo(idx);
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
    searchCount.textContent = "";
    renderOutline();
    rebuildSlots();
    if (viewMode === "single") navigateTo(0, { recordHistory: false });
    else scheduleRender(0);
    renderThumbsShell();
    loadAttachments();
    setStatus(`${label} — ${pageCount} pages`);
    updateChrome();
    void refreshUndoUi();
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

function findSlotAtClient(clientX: number, clientY: number): { slot: Slot; idx: number } | null {
  let best: { slot: Slot; idx: number; dist: number } | null = null;
  for (let i = 0; i < slots.length; i++) {
    const s = slots[i];
    if (!s) continue;
    const r = s.wrap.getBoundingClientRect();
    if (r.width <= 0 || r.height <= 0) continue;
    if (clientX >= r.left && clientX <= r.right && clientY >= r.top && clientY <= r.bottom) {
      return { slot: s, idx: i };
    }
    const cx = (r.left + r.right) / 2;
    const cy = (r.top + r.bottom) / 2;
    const d = Math.hypot(clientX - cx, clientY - cy);
    if (!best || d < best.dist) best = { slot: s, idx: i, dist: d };
  }
  return best ? { slot: best.slot, idx: best.idx } : null;
}

function applyZoomAround(newZoom: number, clientX: number, clientY: number) {
  if (!currentDoc) return;
  const clamped = clampZoom(newZoom);
  if (clamped === zoom) return;
  const anchor = findSlotAtClient(clientX, clientY);
  let fx = 0.5, fy = 0.5;
  let anchorSlot: Slot | null = null;
  if (anchor) {
    const r = anchor.slot.wrap.getBoundingClientRect();
    fx = (clientX - r.left) / r.width;
    fy = (clientY - r.top) / r.height;
    anchorSlot = anchor.slot;
  }
  fitMode = "custom";
  fitSelect.value = "custom";
  zoom = clamped;
  invalidateRenders();
  sizeSlots();
  if (viewMode === "single") {
    applyViewMode();
    scheduleRender(currentPage);
  } else {
    for (const [i, slot] of slots.entries()) {
      const rect = slot.wrap.getBoundingClientRect();
      const vRect = viewer.getBoundingClientRect();
      const visible = rect.bottom > vRect.top - 400 && rect.top < vRect.bottom + 400;
      if (visible) scheduleRender(i);
    }
  }
  if (anchorSlot) {
    const nr = anchorSlot.wrap.getBoundingClientRect();
    const wantedLeft = clientX - nr.width * fx;
    const wantedTop = clientY - nr.height * fy;
    viewer.scrollLeft += nr.left - wantedLeft;
    viewer.scrollTop += nr.top - wantedTop;
  }
  updateChrome();
}

let wheelZoomMode = false;
function setWheelZoomMode(on: boolean) {
  wheelZoomMode = on;
  document.body.classList.toggle("wheelZoomMode", on);
  if (!on) document.body.classList.remove("panning");
  const btn = document.getElementById("wheelZoomBtn");
  if (btn) btn.classList.toggle("active", on);
  if (currentDoc) {
    sizeSlots();
    const slot = slots[currentPage];
    if (slot) {
      slot.wrap.scrollIntoView({ block: "start", inline: "center" });
    } else if (on) {
      recenterScrollX();
    }
  }
}
document.getElementById("wheelZoomBtn")!.addEventListener("click", () => {
  setWheelZoomMode(!wheelZoomMode);
});

type PanState = {
  pointerId: number;
  startClientX: number;
  startClientY: number;
  startScrollLeft: number;
  startScrollTop: number;
};
let panState: PanState | null = null;

viewer.addEventListener("pointerdown", (e: PointerEvent) => {
  if (!wheelZoomMode) return;
  if (e.button !== 0) return;
  e.preventDefault();
  panState = {
    pointerId: e.pointerId,
    startClientX: e.clientX,
    startClientY: e.clientY,
    startScrollLeft: viewer.scrollLeft,
    startScrollTop: viewer.scrollTop,
  };
  viewer.setPointerCapture(e.pointerId);
  document.body.classList.add("panning");
});

viewer.addEventListener("pointermove", (e: PointerEvent) => {
  if (!panState || e.pointerId !== panState.pointerId) return;
  viewer.scrollLeft = panState.startScrollLeft - (e.clientX - panState.startClientX);
  viewer.scrollTop = panState.startScrollTop - (e.clientY - panState.startClientY);
});

function endPan(e: PointerEvent) {
  if (!panState || e.pointerId !== panState.pointerId) return;
  try { viewer.releasePointerCapture(panState.pointerId); } catch { /* ignore */ }
  panState = null;
  document.body.classList.remove("panning");
}
viewer.addEventListener("pointerup", endPan);
viewer.addEventListener("pointercancel", endPan);

viewer.addEventListener(
  "wheel",
  (e: WheelEvent) => {
    if (!currentDoc) return;
    if (e.ctrlKey || wheelZoomMode) {
      e.preventDefault();
      const step = e.ctrlKey ? 0.1 : 0.15;
      const delta = e.deltaY > 0 ? -step : step;
      applyZoomAround(zoom + delta, e.clientX, e.clientY);
      return;
    }
    if (viewMode !== "single") return;
    const cs = getComputedStyle(pageList);
    const padTop = parseFloat(cs.paddingTop) || 0;
    const padBottom = parseFloat(cs.paddingBottom) || 0;
    const atTop = viewer.scrollTop <= padTop + 1;
    const atBottom =
      viewer.scrollTop + viewer.clientHeight >= viewer.scrollHeight - padBottom - 1;
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
  const prevScrollTop = viewer.scrollTop;
  const prevScrollLeft = viewer.scrollLeft;
  const prevPageCount = pageCount;
  const prevPageSizes = pageSizes;
  const sameGeometry =
    prevPageCount === snap.page_count &&
    prevPageSizes.length === snap.page_sizes.length &&
    prevPageSizes.every(
      (p, i) =>
        p.width_pt === snap.page_sizes[i].width_pt &&
        p.height_pt === snap.page_sizes[i].height_pt,
    );

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
  if (viewMode === "single") {
    navigateTo(currentPage, { recordHistory: false });
  } else if (sameGeometry) {
    // Annotation-only edits: keep the user's scroll position instead of jumping.
    viewer.scrollTop = prevScrollTop;
    viewer.scrollLeft = prevScrollLeft;
    scheduleRender(currentPage);
  } else {
    scheduleRender(currentPage);
  }
  renderThumbsShell();
  updateChrome();
  void refreshUndoUi();
  void notifyAnnotationsChanged();
}

let notifyAnnotationsChanged: () => Promise<void> = async () => {};

type UndoStatus = { undo: number; redo: number };
const undoBtn = document.getElementById("undoBtn") as HTMLButtonElement;
const redoBtn = document.getElementById("redoBtn") as HTMLButtonElement;

async function refreshUndoUi() {
  if (!currentDoc) {
    undoBtn.disabled = true;
    redoBtn.disabled = true;
    return;
  }
  try {
    const s = await invoke<UndoStatus>("undo_status", { document: currentDoc });
    undoBtn.disabled = s.undo === 0;
    redoBtn.disabled = s.redo === 0;
  } catch {
    undoBtn.disabled = true;
    redoBtn.disabled = true;
  }
}

async function doUndo() {
  if (!currentDoc || undoBtn.disabled) return;
  setStatus("Undo…");
  try {
    const snap = await invoke<DocSnapshot | null>("undo_doc", { document: currentDoc });
    if (!snap) {
      setStatus("Nothing to undo");
      return;
    }
    clearSelection();
    applyDocSnapshot(snap);
    setStatus("Undone");
  } catch (e) {
    setStatus(`Undo error: ${e}`);
  }
}

async function doRedo() {
  if (!currentDoc || redoBtn.disabled) return;
  setStatus("Redo…");
  try {
    const snap = await invoke<DocSnapshot | null>("redo_doc", { document: currentDoc });
    if (!snap) {
      setStatus("Nothing to redo");
      return;
    }
    clearSelection();
    applyDocSnapshot(snap);
    setStatus("Redone");
  } catch (e) {
    setStatus(`Redo error: ${e}`);
  }
}

undoBtn.addEventListener("click", () => void doUndo());
redoBtn.addEventListener("click", () => void doRedo());

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
    const base = selectedPages.size === 0 ? "" : `${selectedPages.size} selected`;
    modeHint.textContent = editMode === "delete"
      ? (base ? `⚠ Delete mode — ${base}` : "⚠ Delete mode")
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
  const level = pdfaLevelEl?.value ?? "off";
  if (level !== "off") {
    setStatus(`Saving as PDF/A-${level}…`);
    try {
      const label = await invoke<string>("save_as_pdfa", {
        document: currentDoc,
        args: { path: dest, level },
      });
      setStatus(`Saved as ${label}: ${dest}`);
    } catch (e) {
      setStatus(`PDF/A save error: ${e}`);
    }
    return;
  }
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
  document.querySelectorAll(".ribbonTab[data-ribbon-tab]").forEach((el) => {
    el.classList.toggle("active", (el as HTMLElement).dataset.ribbonTab === tab);
  });
  const wasOrganize = document.body.classList.contains("organize");
  const wasConvert = document.body.classList.contains("convert");
  if (mergeMode && tab !== "organize") exitMergeMode();
  if (tab !== "convert" && wasConvert) exitConvertMode();
  if (tab === "convert" && !wasConvert) enterConvertMode();
  document.body.classList.toggle("organize", tab === "organize");
  if (tab === "organize") {
    // Show sidebar so the full-width thumbs grid is visible.
    document.getElementById("sidebar")!.classList.remove("hidden");
    setEditMode("select");
  } else if (tab === "reader") {
    setEditMode("select");
    clearSelection();
  }
  // Viewer was hidden during organize/convert so fit-mode zoom is stale — recompute.
  if ((wasOrganize || wasConvert) && tab !== "organize" && tab !== "convert" && currentDoc) {
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

document.querySelectorAll(".ribbonTab[data-ribbon-tab]").forEach((el) => {
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

const insertMenuBtn = document.getElementById("insertMenuBtn") as HTMLButtonElement;
const insertMenu = document.getElementById("insertMenu") as HTMLDivElement;
function setInsertMenuOpen(open: boolean) {
  insertMenu.classList.toggle("hidden", !open);
  insertMenuBtn.setAttribute("aria-expanded", String(open));
}
insertMenuBtn.addEventListener("click", (e) => {
  e.stopPropagation();
  setInsertMenuOpen(insertMenu.classList.contains("hidden"));
});
document.addEventListener("click", (e) => {
  if (!insertMenu.classList.contains("hidden")
      && !insertMenu.contains(e.target as Node)
      && e.target !== insertMenuBtn) {
    setInsertMenuOpen(false);
  }
});
type InsertWhere = "before" | "after" | "end";
async function runInsertPdf(where: InsertWhere) {
  setInsertMenuOpen(false);
  if (!currentDoc) { setStatus("Open a PDF first"); return; }
  const picked = await open({
    multiple: true,
    filters: [{ name: "PDF", extensions: ["pdf"] }],
  });
  if (!picked) return;
  const paths = Array.isArray(picked) ? picked : [picked];
  if (paths.length === 0) return;
  const at = where === "end" ? pageCount
    : where === "before" ? currentPage
    : currentPage + 1;
  setStatus(`Inserting ${paths.length} file${paths.length === 1 ? "" : "s"}…`);
  try {
    const snap = await invoke<DocSnapshot>("insert_pdf_files", {
      document: currentDoc, paths, atIndex: at,
    });
    clearSelection();
    applyDocSnapshot(snap);
    setStatus(`Inserted ${paths.length} file${paths.length === 1 ? "" : "s"} (${snap.page_count} pages total)`);
  } catch (err) {
    setStatus(`Insert error: ${err}`);
  }
}
document.getElementById("insertBeforeBtn")!.addEventListener("click", () => runInsertPdf("before"));
document.getElementById("insertAfterBtn")!.addEventListener("click", () => runInsertPdf("after"));
document.getElementById("insertEndBtn")!.addEventListener("click", () => runInsertPdf("end"));
document.getElementById("mergePdfsBtn")!.addEventListener("click", () => enterMergeMode());
document.getElementById("splitPdfBtn")!.addEventListener("click", () => void runSplitPdf());

const wmOverlay = document.getElementById("wmOverlay") as HTMLDivElement;
const hfOverlay = document.getElementById("hfOverlay") as HTMLDivElement;
const wmSectionText = document.getElementById("wmSectionText") as HTMLDivElement;
const wmSectionImage = document.getElementById("wmSectionImage") as HTMLDivElement;
const wmTabText = document.getElementById("wmTabText") as HTMLButtonElement;
const wmTabImage = document.getElementById("wmTabImage") as HTMLButtonElement;
let wmMode: "text" | "image" = "text";
function setWmMode(mode: "text" | "image") {
  wmMode = mode;
  wmTabText.classList.toggle("active", mode === "text");
  wmTabImage.classList.toggle("active", mode === "image");
  wmSectionText.classList.toggle("hidden", mode !== "text");
  wmSectionImage.classList.toggle("hidden", mode !== "image");
}
wmTabText.addEventListener("click", () => setWmMode("text"));
wmTabImage.addEventListener("click", () => setWmMode("image"));

document.getElementById("watermarkBtn")!.addEventListener("click", () => {
  if (!currentDoc) { setStatus("Open a PDF first"); return; }
  wmOverlay.classList.remove("hidden");
  if (wmMode === "text") (document.getElementById("wmText") as HTMLInputElement).focus();
});
document.getElementById("wmClose")!.addEventListener("click", () => wmOverlay.classList.add("hidden"));
document.getElementById("wmCancel")!.addEventListener("click", () => wmOverlay.classList.add("hidden"));
document.getElementById("wmCommit")!.addEventListener("click", () => void runWatermark());
document.getElementById("iwPick")!.addEventListener("click", () => void pickImageWatermark());

async function pickImageWatermark() {
  const picked = await open({
    multiple: false,
    directory: false,
    filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp", "bmp"] }],
    title: "Pick image for watermark",
  });
  if (picked && typeof picked === "string") {
    (document.getElementById("iwPath") as HTMLInputElement).value = picked;
  }
}

async function runWatermark() {
  if (!currentDoc) return;
  if (wmMode === "text") {
    const text = (document.getElementById("wmText") as HTMLInputElement).value.trim();
    if (!text) { setStatus("Watermark text is empty"); return; }
    const args = {
      text,
      font_size: parseFloat((document.getElementById("wmFontSize") as HTMLInputElement).value) || 72,
      rotation_deg: parseFloat((document.getElementById("wmRot") as HTMLInputElement).value) || 0,
      opacity: parseFloat((document.getElementById("wmOpacity") as HTMLInputElement).value) || 0.25,
      gray: parseFloat((document.getElementById("wmGray") as HTMLInputElement).value),
    };
    if (!Number.isFinite(args.gray)) args.gray = 0.5;
    wmOverlay.classList.add("hidden");
    setStatus("Applying watermark…");
    try {
      const snap = await invoke<DocSnapshot>("add_watermark", { document: currentDoc, args });
      applyDocSnapshot(snap);
      setStatus("Watermark applied");
    } catch (e) {
      setStatus(`Watermark error: ${e}`);
    }
  } else {
    const image_path = (document.getElementById("iwPath") as HTMLInputElement).value.trim();
    if (!image_path) { setStatus("Pick an image first"); return; }
    const args = {
      image_path,
      scale_frac: parseFloat((document.getElementById("iwScale") as HTMLInputElement).value) || 0.4,
      rotation_deg: parseFloat((document.getElementById("iwRot") as HTMLInputElement).value) || 0,
      opacity: parseFloat((document.getElementById("iwOpacity") as HTMLInputElement).value) || 0.5,
      anchor: (document.getElementById("iwAnchor") as HTMLSelectElement).value || "center",
      margin_pt: parseFloat((document.getElementById("iwMargin") as HTMLInputElement).value) || 36,
    };
    wmOverlay.classList.add("hidden");
    setStatus("Applying image watermark…");
    try {
      const snap = await invoke<DocSnapshot>("add_image_watermark", { document: currentDoc, args });
      applyDocSnapshot(snap);
      setStatus("Image watermark applied");
    } catch (e) {
      setStatus(`Image watermark error: ${e}`);
    }
  }
}

document.getElementById("headerFooterBtn")!.addEventListener("click", () => {
  if (!currentDoc) { setStatus("Open a PDF first"); return; }
  hfOverlay.classList.remove("hidden");
});
document.getElementById("hfClose")!.addEventListener("click", () => hfOverlay.classList.add("hidden"));
document.getElementById("hfCancel")!.addEventListener("click", () => hfOverlay.classList.add("hidden"));
document.getElementById("hfCommit")!.addEventListener("click", () => void runHeaderFooter());

async function runHeaderFooter() {
  if (!currentDoc) return;
  const v = (id: string) => (document.getElementById(id) as HTMLInputElement).value;
  const args = {
    header_left: v("hfHL"),
    header_center: v("hfHC"),
    header_right: v("hfHR"),
    footer_left: v("hfFL"),
    footer_center: v("hfFC"),
    footer_right: v("hfFR"),
    font_size: parseFloat(v("hfFontSize")) || 10,
    margin_pt: parseFloat(v("hfMargin")) || 24,
    first_page_number: parseInt(v("hfFirst"), 10) || 1,
    gray: parseFloat(v("hfGray")),
  };
  if (!Number.isFinite(args.gray)) args.gray = 0;
  const anyText = [args.header_left, args.header_center, args.header_right, args.footer_left, args.footer_center, args.footer_right].some((s) => s.trim() !== "");
  if (!anyText) { setStatus("Fill at least one slot"); return; }
  hfOverlay.classList.add("hidden");
  setStatus("Applying header/footer…");
  try {
    const snap = await invoke<DocSnapshot>("add_header_footer", { document: currentDoc, args });
    applyDocSnapshot(snap);
    setStatus("Header/footer applied");
  } catch (e) {
    setStatus(`Header/footer error: ${e}`);
  }
}

async function runSplitPdf() {
  if (!currentDoc) { setStatus("Open a PDF first"); return; }
  const suggestion = `1-${Math.min(pageCount, Math.max(1, Math.ceil(pageCount / 2)))},${Math.min(pageCount, Math.max(1, Math.ceil(pageCount / 2)) + 1)}-${pageCount}`;
  const spec = window.prompt(
    `Split into parts by page ranges (1-based, comma-separated).\nExamples:  1-3,4-6,7-${pageCount}    or    1,2-5,6`,
    suggestion,
  );
  if (!spec) return;
  const outDir = await open({ directory: true, title: "Output folder for split parts" });
  if (!outDir || typeof outDir !== "string") return;
  const baseName = currentDocPath
    ? currentDocPath.replace(/\\/g, "/").split("/").pop()!.replace(/\.pdf$/i, "")
    : "document";
  setStatus("Splitting…");
  try {
    const res = await invoke<{ files: string[] }>("split_pdf", {
      document: currentDoc,
      ranges: spec,
      outDir,
      baseName,
    });
    setStatus(`Split → ${res.files.length} file${res.files.length === 1 ? "" : "s"} in ${outDir}`);
  } catch (e) {
    setStatus(`Split error: ${e}`);
  }
}

// ---------- Merge workspace ----------
type MergeFile = { path: string; name: string };

// ----------------- Convert pane (PDF → PDF/A) -----------------

let convertFiles: MergeFile[] = [];
let convertOutDir: string | null = null;
const convertPane = document.getElementById("convertPane") as HTMLDivElement;
const convertList = document.getElementById("convertList") as HTMLDivElement;
const convertCommitBtn = document.getElementById("convertCommitBtn") as HTMLButtonElement;
const convertLevelEl = document.getElementById("convertLevel") as HTMLSelectElement;
const convertOutLabel = document.getElementById("convertOutLabel") as HTMLSpanElement;

function enterConvertMode() {
  document.body.classList.add("convert");
  // Seed the level dropdown with the current default if it isn't "off".
  if (pdfaLevelEl && pdfaLevelEl.value !== "off") convertLevelEl.value = pdfaLevelEl.value;
  renderConvertList();
  updateConvertOutLabel();
}
function exitConvertMode() {
  document.body.classList.remove("convert");
  convertFiles = [];
  renderConvertList();
}
function renderConvertList() {
  convertList.innerHTML = "";
  if (convertFiles.length === 0) {
    const empty = document.createElement("div");
    empty.className = "mergeEmpty";
    empty.textContent = "No files yet. Click ＋ to add PDFs to convert.";
    convertList.appendChild(empty);
  } else {
    convertFiles.forEach((f, i) => {
      const tile = document.createElement("div");
      tile.className = "mergeTile";
      tile.dataset.idx = String(i);
      tile.innerHTML = `
        <span class="mergeIndex">${i + 1}</span>
        <button class="mergeRemove" title="Remove" data-idx="${i}">✕</button>
        <div class="mergeName">${f.name}</div>
        <div class="mergePath">${f.path}</div>`;
      convertList.appendChild(tile);
    });
  }
  convertCommitBtn.disabled = convertFiles.length === 0 || !convertOutDir;
}
function updateConvertOutLabel() {
  convertOutLabel.textContent = convertOutDir
    ? `Output: ${convertOutDir}`
    : "Output: (none chosen — pick an output folder)";
}
function addConvertPaths(paths: string[]) {
  const pdfs = paths.filter((p) => /\.pdf$/i.test(p));
  for (const p of pdfs) {
    if (convertFiles.some((f) => f.path === p)) continue;
    convertFiles.push({ path: p, name: fileNameFromPath(p) });
  }
  renderConvertList();
}

document.getElementById("convertAddBtn")!.addEventListener("click", async () => {
  const picked = await open({
    multiple: true,
    filters: [{ name: "PDF", extensions: ["pdf"] }],
  });
  if (!picked) return;
  addConvertPaths(Array.isArray(picked) ? picked : [picked]);
});
document.getElementById("convertOutBtn")!.addEventListener("click", async () => {
  const picked = await open({ directory: true, multiple: false, title: "Pick output folder" });
  if (!picked || typeof picked !== "string") return;
  convertOutDir = picked;
  updateConvertOutLabel();
  renderConvertList();
});
document.getElementById("convertCancelBtn")!.addEventListener("click", () => setRibbonTab("reader"));
convertList.addEventListener("click", (e) => {
  const btn = (e.target as HTMLElement).closest(".mergeRemove") as HTMLElement | null;
  if (!btn) return;
  const idx = parseInt(btn.dataset.idx || "-1", 10);
  if (idx < 0) return;
  convertFiles.splice(idx, 1);
  renderConvertList();
});
convertCommitBtn.addEventListener("click", async () => {
  if (convertFiles.length === 0 || !convertOutDir) return;
  const paths = convertFiles.map((f) => f.path);
  const level = convertLevelEl.value;
  setStatus(`Converting ${paths.length} file${paths.length === 1 ? "" : "s"} to PDF/A-${level}…`);
  try {
    type ConvertResult = { files: string[]; failed: string[] };
    const res = await invoke<ConvertResult>("convert_files_to_pdfa", {
      paths,
      outDir: convertOutDir,
      level,
    });
    const okCount = res.files.length;
    const failCount = res.failed.length;
    if (failCount > 0) {
      setStatus(`Converted ${okCount}/${paths.length}. Failed: ${res.failed.map(fileNameFromPath).join(", ")}`);
    } else {
      setStatus(`Converted ${okCount} file${okCount === 1 ? "" : "s"} → ${convertOutDir}`);
    }
    convertFiles = [];
    renderConvertList();
  } catch (e) {
    setStatus(`Convert error: ${e}`);
  }
});

let mergeMode = false;
let mergeFiles: MergeFile[] = [];
const mergePane = document.getElementById("mergePane") as HTMLDivElement;
const mergeList = document.getElementById("mergeList") as HTMLDivElement;
const mergeCommitBtn = document.getElementById("mergeCommitBtn") as HTMLButtonElement;

function enterMergeMode() {
  mergeMode = true;
  mergeFiles = [];
  document.body.classList.add("merge");
  setRibbonTab("organize");
  renderMergeList();
}
function exitMergeMode() {
  mergeMode = false;
  mergeFiles = [];
  document.body.classList.remove("merge", "mergeDrop");
  renderMergeList();
}
function fileNameFromPath(p: string): string {
  const m = p.replace(/\\/g, "/").split("/").pop() ?? p;
  return m || p;
}
function renderMergeList() {
  mergeList.innerHTML = "";
  if (mergeFiles.length === 0) {
    const empty = document.createElement("div");
    empty.className = "mergeEmpty";
    empty.textContent = "No files yet. Drop PDFs here or click ＋ Add files.";
    mergeList.appendChild(empty);
  } else {
    mergeFiles.forEach((f, i) => {
      const tile = document.createElement("div");
      tile.className = "mergeTile";
      tile.dataset.idx = String(i);
      tile.innerHTML = `
        <span class="mergeIndex">${i + 1}</span>
        <button class="mergeRemove" title="Remove" data-idx="${i}">✕</button>
        <div class="mergeName">${f.name}</div>
        <div class="mergePath">${f.path}</div>`;
      mergeList.appendChild(tile);
    });
  }
  mergeCommitBtn.disabled = mergeFiles.length < 2;
}
function addMergePaths(paths: string[]) {
  const pdfs = paths.filter((p) => /\.pdf$/i.test(p));
  for (const p of pdfs) {
    if (mergeFiles.some((f) => f.path === p)) continue;
    mergeFiles.push({ path: p, name: fileNameFromPath(p) });
  }
  renderMergeList();
}

document.getElementById("mergeAddBtn")!.addEventListener("click", async () => {
  const picked = await open({
    multiple: true,
    filters: [{ name: "PDF", extensions: ["pdf"] }],
  });
  if (!picked) return;
  const paths = Array.isArray(picked) ? picked : [picked];
  addMergePaths(paths);
});
document.getElementById("mergeCancelBtn")!.addEventListener("click", exitMergeMode);
mergeCommitBtn.addEventListener("click", async () => {
  if (mergeFiles.length < 2) return;
  const paths = mergeFiles.map((f) => f.path);
  setStatus(`Merging ${paths.length} files…`);
  try {
    type MergeResult = OpenedDocument & { temp_path: string };
    const res = await invoke<MergeResult>("merge_files_and_open", { paths });
    const tmp = res.temp_path;
    exitMergeMode();
    applyOpenedDoc(res, tmp);
    setRibbonTab("organize");
    setStatus(`Merged ${paths.length} files into ${res.page_count} pages (temp: ${tmp})`);
  } catch (err) {
    setStatus(`Merge error: ${err}`);
  }
});

// remove-button delegation
mergeList.addEventListener("click", (e) => {
  const btn = (e.target as HTMLElement).closest(".mergeRemove") as HTMLElement | null;
  if (!btn) return;
  const idx = parseInt(btn.dataset.idx || "-1", 10);
  if (idx < 0) return;
  mergeFiles.splice(idx, 1);
  renderMergeList();
});

// pointer-drag reorder of merge tiles
type MDrag = {
  tracking: boolean; active: boolean; startX: number; startY: number;
  startIdx: number; preview: HTMLElement | null;
  target: HTMLElement | null; after: boolean;
};
let mdrag: MDrag = {
  tracking: false, active: false, startX: 0, startY: 0,
  startIdx: -1, preview: null, target: null, after: false,
};
function resetMdrag() {
  if (mdrag.preview) mdrag.preview.remove();
  document.querySelectorAll(".mergeTile.dragging, .mergeTile.dropBefore, .mergeTile.dropAfter")
    .forEach((el) => el.classList.remove("dragging", "dropBefore", "dropAfter"));
  mdrag = {
    tracking: false, active: false, startX: 0, startY: 0,
    startIdx: -1, preview: null, target: null, after: false,
  };
}
function findMergeDropTarget(x: number, y: number): { el: HTMLElement; after: boolean } | null {
  const el = document.elementFromPoint(x, y) as HTMLElement | null;
  const hit = el?.closest(".mergeTile") as HTMLElement | null;
  const all = Array.from(mergeList.querySelectorAll<HTMLElement>(".mergeTile"));
  if (hit) {
    const r = hit.getBoundingClientRect();
    return { el: hit, after: x > r.left + r.width / 2 };
  }
  let best: HTMLElement | null = null;
  let bestRect: DOMRect | null = null;
  let bestDist = Infinity;
  for (const t of all) {
    if (t.classList.contains("dragging")) continue;
    const r = t.getBoundingClientRect();
    const cx = r.left + r.width / 2;
    const cy = r.top + r.height / 2;
    const dx = cx - x;
    const dy = cy - y;
    const d = dx * dx + dy * dy;
    if (d < bestDist) { bestDist = d; best = t; bestRect = r; }
  }
  if (!best || !bestRect) return null;
  return { el: best, after: x > bestRect.left + bestRect.width / 2 };
}
mergeList.addEventListener("pointerdown", (e) => {
  if (e.button !== 0) return;
  const t = (e.target as HTMLElement | null)?.closest(".mergeTile") as HTMLElement | null;
  if (!t) return;
  if ((e.target as HTMLElement).closest(".mergeRemove")) return;
  const idx = parseInt(t.dataset.idx || "-1", 10);
  if (idx < 0) return;
  mdrag = {
    tracking: true, active: false, startX: e.clientX, startY: e.clientY,
    startIdx: idx, preview: null, target: null, after: false,
  };
});
window.addEventListener("pointermove", (e) => {
  if (!mdrag.tracking) return;
  if (!mdrag.active) {
    const dx = e.clientX - mdrag.startX;
    const dy = e.clientY - mdrag.startY;
    if (dx * dx + dy * dy < 25) return;
    mdrag.active = true;
    const tile = mergeList.querySelector<HTMLElement>(`.mergeTile[data-idx="${mdrag.startIdx}"]`);
    tile?.classList.add("dragging");
    const prev = document.createElement("div");
    prev.className = "dragPreview";
    prev.textContent = mergeFiles[mdrag.startIdx]?.name ?? "file";
    document.body.appendChild(prev);
    mdrag.preview = prev;
  }
  if (mdrag.preview) {
    mdrag.preview.style.left = `${e.clientX + 14}px`;
    mdrag.preview.style.top = `${e.clientY + 14}px`;
  }
  const hit = findMergeDropTarget(e.clientX, e.clientY);
  const target = hit?.el ?? null;
  if (mdrag.target && mdrag.target !== target) {
    mdrag.target.classList.remove("dropBefore", "dropAfter");
  }
  mdrag.target = target;
  if (hit && target) {
    mdrag.after = hit.after;
    target.classList.toggle("dropBefore", !mdrag.after);
    target.classList.toggle("dropAfter", mdrag.after);
  }
});
window.addEventListener("pointerup", () => {
  if (!mdrag.tracking) return;
  const wasActive = mdrag.active;
  const from = mdrag.startIdx;
  const target = mdrag.target;
  const after = mdrag.after;
  resetMdrag();
  if (!wasActive || from < 0 || !target) return;
  const toIdxRaw = parseInt(target.dataset.idx || "-1", 10);
  if (toIdxRaw < 0 || toIdxRaw === from) return;
  const moved = mergeFiles[from];
  mergeFiles.splice(from, 1);
  let insertAt = toIdxRaw;
  if (toIdxRaw > from) insertAt -= 1;
  if (after) insertAt += 1;
  insertAt = Math.max(0, Math.min(mergeFiles.length, insertAt));
  mergeFiles.splice(insertAt, 0, moved);
  renderMergeList();
});
window.addEventListener("pointercancel", resetMdrag);
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

// Settings panel
type GhostscriptInfo = { available: boolean; path: string | null; version: string | null };
const settingsOverlay = document.getElementById("settingsOverlay") as HTMLDivElement;
const gsStatusEl = document.getElementById("gsStatus") as HTMLDivElement;
const gsInstallEl = document.getElementById("gsInstall") as HTMLDivElement;
const pdfaLevelEl = document.getElementById("pdfaLevel") as HTMLSelectElement;
let gsInfo: GhostscriptInfo = { available: false, path: null, version: null };

async function loadDefaultPdfa() {
  try {
    const v = await invoke<string>("get_default_pdfa");
    if (["off", "1b", "2b", "3b", "4"].includes(v)) pdfaLevelEl.value = v;
  } catch {}
}
void loadDefaultPdfa();
pdfaLevelEl.addEventListener("change", () => {
  void invoke("set_default_pdfa", { level: pdfaLevelEl.value }).catch(() => {});
});

async function refreshGhostscript() {
  gsStatusEl.textContent = "Checking Ghostscript…";
  gsInstallEl.classList.add("hidden");
  try {
    gsInfo = await invoke<GhostscriptInfo>("check_ghostscript");
    if (gsInfo.available) {
      gsStatusEl.textContent = `Ghostscript ${gsInfo.version ?? ""} — ${gsInfo.path ?? ""}`;
      gsStatusEl.style.color = "#8fcf8f";
    } else {
      gsStatusEl.textContent = "Ghostscript not detected.";
      gsStatusEl.style.color = "#d98a8a";
      gsInstallEl.classList.remove("hidden");
    }
  } catch (e) {
    gsStatusEl.textContent = `Detection error: ${e}`;
    gsStatusEl.style.color = "#d98a8a";
  }
}
void refreshGhostscript();

document.getElementById("settingsBtnTop")!.addEventListener("click", () => {
  settingsOverlay.classList.remove("hidden");
  void refreshGhostscript();
  void refreshVirtualPrinter();
});
document.getElementById("settingsClose")!.addEventListener("click", () => settingsOverlay.classList.add("hidden"));
settingsOverlay.addEventListener("click", (e) => {
  if (e.target === settingsOverlay) settingsOverlay.classList.add("hidden");
});
document.getElementById("gsRecheckBtn")!.addEventListener("click", () => void refreshGhostscript());
document.getElementById("gsLink")!.addEventListener("click", (e) => {
  e.preventDefault();
  void invoke("open_external", { url: "https://ghostscript.com/releases/gsdnld.html" });
});
document.getElementById("pdfaSaveBtn")!.addEventListener("click", () => void runSaveAsPdfa());

async function runSaveAsPdfa() {
  if (!currentDoc) { setStatus("Open a PDF first"); return; }
  if (!gsInfo.available) {
    setStatus("Ghostscript required — install it first");
    return;
  }
  const level = pdfaLevelEl.value === "off" ? "2b" : pdfaLevelEl.value;
  const base = currentDocPath
    ? currentDocPath.replace(/\\/g, "/").split("/").pop()!.replace(/\.pdf$/i, "")
    : "document";
  const dest = await save({
    defaultPath: `${base}_pdfa-${level}.pdf`,
    filters: [{ name: "PDF", extensions: ["pdf"] }],
    title: `Save as PDF/A-${level}`,
  });
  if (!dest || typeof dest !== "string") return;
  settingsOverlay.classList.add("hidden");
  setStatus(`Converting to PDF/A-${level}…`);
  try {
    const label = await invoke<string>("save_as_pdfa", {
      document: currentDoc,
      args: { path: dest, level },
    });
    setStatus(`Saved as ${label}`);
  } catch (e) {
    setStatus(`PDF/A export error: ${e}`);
  }
}

// Virtual printer (Print-to-PDF)
type VirtualPrinterInfo = {
  installed: boolean;
  printer_name: string;
  port_path: string;
  output_dir: string;
  ghostscript_available: boolean;
  ghostscript_version: string | null;
  supported: boolean;
};
const vpStatusEl = document.getElementById("vpStatus") as HTMLDivElement;
const vpPathRowEl = document.getElementById("vpPathRow") as HTMLDivElement;
const vpOutRowEl = document.getElementById("vpOutRow") as HTMLDivElement;
const vpWarnEl = document.getElementById("vpWarn") as HTMLDivElement;
const vpInstallBtn = document.getElementById("vpInstallBtn") as HTMLButtonElement;
const vpUninstallBtn = document.getElementById("vpUninstallBtn") as HTMLButtonElement;
const vpOpenFolderBtn = document.getElementById("vpOpenFolderBtn") as HTMLButtonElement;

async function refreshVirtualPrinter() {
  vpStatusEl.textContent = "Checking virtual printer…";
  vpStatusEl.style.color = "#bbb";
  vpPathRowEl.classList.add("hidden");
  vpWarnEl.classList.add("hidden");
  try {
    const info = await invoke<VirtualPrinterInfo>("virtual_printer_status");
    if (!info.supported) {
      vpStatusEl.textContent = "Virtual printer is only available on Windows.";
      vpStatusEl.style.color = "#d98a8a";
      vpInstallBtn.disabled = true;
      vpUninstallBtn.disabled = true;
      return;
    }
    if (info.installed) {
      vpStatusEl.textContent = `Installed as "${info.printer_name}".`;
      vpStatusEl.style.color = "#8fcf8f";
      vpInstallBtn.disabled = true;
      vpUninstallBtn.disabled = false;
    } else {
      vpStatusEl.textContent = "Not installed.";
      vpStatusEl.style.color = "#d98a8a";
      vpInstallBtn.disabled = false;
      vpUninstallBtn.disabled = true;
    }
    vpPathRowEl.textContent = `Port file: ${info.port_path}`;
    vpPathRowEl.classList.remove("hidden");
    vpOutRowEl.textContent = `Output folder: ${info.output_dir}`;
    if (!info.ghostscript_available) {
      vpWarnEl.classList.remove("hidden");
    }
  } catch (e) {
    vpStatusEl.textContent = `Detection error: ${e}`;
    vpStatusEl.style.color = "#d98a8a";
  }
}

vpInstallBtn.addEventListener("click", async () => {
  vpInstallBtn.disabled = true;
  setStatus("Installing virtual printer (admin prompt will appear)…");
  try {
    await invoke("install_virtual_printer");
    setStatus("Virtual printer installed");
  } catch (e) {
    setStatus(`Install failed: ${e}`);
  }
  await refreshVirtualPrinter();
});

vpUninstallBtn.addEventListener("click", async () => {
  vpUninstallBtn.disabled = true;
  setStatus("Removing virtual printer (admin prompt will appear)…");
  try {
    await invoke("uninstall_virtual_printer");
    setStatus("Virtual printer removed");
  } catch (e) {
    setStatus(`Uninstall failed: ${e}`);
  }
  await refreshVirtualPrinter();
});

vpOpenFolderBtn.addEventListener("click", async () => {
  try {
    await invoke("open_print_output_folder");
  } catch (e) {
    setStatus(`Open folder failed: ${e}`);
  }
});

// Auto-open newly printed PDFs
void listen<{ path: string }>("print-job-ready", async (ev) => {
  const path = ev.payload.path;
  setStatus(`Print job ready — opening ${path}`);
  await openByPath(path);
});
void listen<{ message: string }>("print-job-failed", (ev) => {
  setStatus(`Print job failed: ${ev.payload.message}`);
});

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
  if (!inField && e.ctrlKey && !e.shiftKey && (e.key === "z" || e.key === "Z")) {
    e.preventDefault();
    void doUndo();
    return;
  }
  if (!inField && e.ctrlKey && ((e.key === "y" || e.key === "Y") || (e.shiftKey && (e.key === "z" || e.key === "Z")))) {
    e.preventDefault();
    void doRedo();
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
      if (mergeMode) document.body.classList.add("mergeDrop");
      else dropOverlay.classList.remove("hidden");
    } else if (p.type === "leave") {
      document.body.classList.remove("mergeDrop");
      dropOverlay.classList.add("hidden");
    } else if (p.type === "drop") {
      document.body.classList.remove("mergeDrop");
      dropOverlay.classList.add("hidden");
      const paths = (p as { paths: string[] }).paths;
      if (mergeMode) {
        addMergePaths(paths);
        return;
      }
      const pdf = paths.find((f) => /\.pdf$/i.test(f));
      if (pdf) await openByPath(pdf);
    }
  })
  .catch(() => {});

// ----------------- Annotation tools (markup + shapes) -----------------

type MarkupKind = "highlight" | "underline" | "strikeout";
type ShapeKindUi = "rect" | "circle" | "line" | "arrow" | "freetext" | "ink" | "stamp" | "region" | "note";
let annotTool: MarkupKind | null = null;
let shapeTool: ShapeKindUi | null = null;
let stampImagePath: string | null = null;
const annotColorInput = document.getElementById("annotColor") as HTMLInputElement;

let printAnnotations = true;
const printToggleBtn = document.getElementById("printToggleBtn") as HTMLButtonElement;
printToggleBtn.addEventListener("click", () => {
  printAnnotations = !printAnnotations;
  printToggleBtn.classList.toggle("active", printAnnotations);
  printToggleBtn.title = printAnnotations
    ? "Include new annotations when printing (toggle)"
    : "Skip new annotations when printing (toggle)";
  setStatus(printAnnotations ? "New annotations will print" : "New annotations will NOT print");
});

function setAnnotTool(k: MarkupKind | null) {
  annotTool = k;
  if (k !== null) shapeTool = null;
  document.querySelectorAll<HTMLButtonElement>(".annotTool").forEach((b) => {
    b.classList.toggle("active", b.dataset.annot === k);
  });
  document.querySelectorAll<HTMLButtonElement>(".shapeTool").forEach((b) => {
    b.classList.toggle("active", false);
  });
  document.body.classList.toggle("annotMode", k !== null);
  document.body.classList.toggle("shapeMode", false);
}

function setShapeTool(k: ShapeKindUi | null) {
  shapeTool = k;
  if (k !== null) annotTool = null;
  document.querySelectorAll<HTMLButtonElement>(".shapeTool").forEach((b) => {
    b.classList.toggle("active", b.dataset.shape === k);
  });
  document.querySelectorAll<HTMLButtonElement>(".annotTool").forEach((b) => {
    b.classList.toggle("active", false);
  });
  document.body.classList.toggle("shapeMode", k !== null);
  document.body.classList.toggle("annotMode", false);
}

document.querySelectorAll<HTMLButtonElement>(".annotTool").forEach((b) => {
  b.addEventListener("click", () => {
    const k = b.dataset.annot as MarkupKind;
    setAnnotTool(annotTool === k ? null : k);
  });
});

document.querySelectorAll<HTMLButtonElement>(".shapeTool").forEach((b) => {
  b.addEventListener("click", async () => {
    const k = b.dataset.shape as ShapeKindUi;
    if (shapeTool === k) {
      setShapeTool(null);
      return;
    }
    if (k === "stamp") {
      const picked = await open({
        multiple: false,
        directory: false,
        filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp", "bmp"] }],
        title: "Pick stamp image",
      });
      if (!picked || typeof picked !== "string") return;
      stampImagePath = picked;
    }
    setShapeTool(k);
  });
});

function hexToRgb01(hex: string): [number, number, number] {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) return [1, 0.95, 0];
  const v = parseInt(m[1], 16);
  return [((v >> 16) & 0xff) / 255, ((v >> 8) & 0xff) / 255, (v & 0xff) / 255];
}

function pageOfRect(rect: DOMRect): { idx: number; slot: Slot } | null {
  const cx = rect.left + rect.width / 2;
  const cy = rect.top + rect.height / 2;
  for (let i = 0; i < slots.length; i++) {
    const s = slots[i];
    if (!s || !s.loaded) continue;
    const bb = s.wrap.getBoundingClientRect();
    if (cx >= bb.left && cx <= bb.right && cy >= bb.top && cy <= bb.bottom) {
      return { idx: i, slot: s };
    }
  }
  return null;
}

function screenRectToPagePt(domRect: DOMRect, slot: Slot, pageIdx: number): RectPt | null {
  const sz = pageSizes[pageIdx];
  if (!sz) return null;
  const bb = slot.wrap.getBoundingClientRect();
  const imgW = bb.width;
  const imgH = bb.height;
  if (imgW <= 0 || imgH <= 0) return null;
  const left = domRect.left - bb.left;
  const top = domRect.top - bb.top;
  const right = left + domRect.width;
  const bottom = top + domRect.height;
  const rot = effectiveRotation(pageIdx);
  const dw = rot === 1 || rot === 3 ? sz.height_pt : sz.width_pt;
  const dh = rot === 1 || rot === 3 ? sz.width_pt : sz.height_pt;
  const sx = imgW / dw;
  const sy = imgH / dh;
  const nx0 = left / sx;
  const ny0 = top / sy;
  const nx1 = right / sx;
  const ny1 = bottom / sy;
  const w0 = sz.width_pt;
  const h0 = sz.height_pt;
  switch (rot) {
    case 0: return { x0: nx0, y0: ny0, x1: nx1, y1: ny1 };
    case 1: return { x0: ny0, y0: h0 - nx1, x1: ny1, y1: h0 - nx0 };
    case 2: return { x0: w0 - nx1, y0: h0 - ny1, x1: w0 - nx0, y1: h0 - ny0 };
    case 3: return { x0: w0 - ny1, y0: nx0, x1: w0 - ny0, y1: nx1 };
    default: return { x0: nx0, y0: ny0, x1: nx1, y1: ny1 };
  }
}

async function applyAnnotationFromSelection() {
  if (!annotTool || !currentDoc) return;
  const sel = window.getSelection();
  if (!sel || sel.isCollapsed || sel.rangeCount === 0) return;
  const byPage = new Map<number, RectPt[]>();
  for (let i = 0; i < sel.rangeCount; i++) {
    const range = sel.getRangeAt(i);
    const rects = Array.from(range.getClientRects());
    for (const r of rects) {
      if (r.width < 1 || r.height < 1) continue;
      const hit = pageOfRect(r);
      if (!hit) continue;
      const pt = screenRectToPagePt(r, hit.slot, hit.idx);
      if (!pt) continue;
      let arr = byPage.get(hit.idx);
      if (!arr) { arr = []; byPage.set(hit.idx, arr); }
      arr.push(pt);
    }
  }
  if (byPage.size === 0) return;
  const color = hexToRgb01(annotColorInput.value);
  const opacity = annotTool === "highlight" ? 0.4 : 1.0;
  const kind = annotTool;
  const pages = Array.from(byPage.keys()).sort((a, b) => a - b);
  setStatus(`Applying ${kind}…`);
  try {
    let snap: DocSnapshot | null = null;
    for (const page of pages) {
      const rects = byPage.get(page)!;
      snap = await invoke<DocSnapshot>("add_annotation", {
        document: currentDoc,
        args: { page, rects, kind, color, opacity, print: printAnnotations },
      });
    }
    sel.removeAllRanges();
    if (snap) applyDocSnapshot(snap);
    setStatus(`${kind} applied`);
  } catch (e) {
    setStatus(`${kind} error: ${e}`);
  }
}

document.addEventListener("mouseup", (ev) => {
  if (!annotTool) return;
  const t = ev.target as Node | null;
  if (!t || !pageList.contains(t)) return;
  setTimeout(() => { void applyAnnotationFromSelection(); }, 0);
});

// ----------------- Shape drawing pipeline -----------------

type DrawState = {
  pageIdx: number;
  slot: Slot;
  startPx: { x: number; y: number };
  currentPx: { x: number; y: number };
  preview: SVGElement;
  pointerId: number;
  /// Ink-only: live stroke points in image px.
  inkPoints?: { x: number; y: number }[];
};
let drawState: DrawState | null = null;

function imagePxFromEvent(slot: Slot, ev: PointerEvent): { x: number; y: number } {
  const bb = slot.wrap.getBoundingClientRect();
  return { x: ev.clientX - bb.left, y: ev.clientY - bb.top };
}

function ensureDrawLayerViewBox(slot: Slot) {
  const bb = slot.wrap.getBoundingClientRect();
  slot.drawLayer.setAttribute("viewBox", `0 0 ${bb.width} ${bb.height}`);
  slot.drawLayer.setAttribute("width", `${bb.width}`);
  slot.drawLayer.setAttribute("height", `${bb.height}`);
}

function makePreviewElement(kind: ShapeKindUi, color: string): SVGElement {
  const ns = "http://www.w3.org/2000/svg";
  if (kind === "rect" || kind === "freetext" || kind === "stamp" || kind === "region") {
    const el = document.createElementNS(ns, "rect");
    el.setAttribute("fill", kind === "region" ? "rgba(74,140,255,0.12)" : "none");
    el.setAttribute("stroke", kind === "region" ? "#4a8cff" : color);
    el.setAttribute("stroke-width", "2");
    if (kind !== "rect") el.setAttribute("stroke-dasharray", "5 4");
    return el;
  }
  if (kind === "circle") {
    const el = document.createElementNS(ns, "ellipse");
    el.setAttribute("fill", "none");
    el.setAttribute("stroke", color);
    el.setAttribute("stroke-width", "2");
    return el;
  }
  if (kind === "line") {
    const el = document.createElementNS(ns, "line");
    el.setAttribute("stroke", color);
    el.setAttribute("stroke-width", "2");
    return el;
  }
  if (kind === "arrow") {
    const g = document.createElementNS(ns, "g");
    const ln = document.createElementNS(ns, "line");
    ln.setAttribute("stroke", color);
    ln.setAttribute("stroke-width", "2");
    g.appendChild(ln);
    const head = document.createElementNS(ns, "polygon");
    head.setAttribute("fill", color);
    head.setAttribute("stroke", color);
    g.appendChild(head);
    return g;
  }
  // ink: polyline grows during pointermove
  const el = document.createElementNS(ns, "polyline");
  el.setAttribute("fill", "none");
  el.setAttribute("stroke", color);
  el.setAttribute("stroke-width", "2");
  el.setAttribute("stroke-linecap", "round");
  el.setAttribute("stroke-linejoin", "round");
  return el;
}

function updatePreview(kind: ShapeKindUi, el: SVGElement, s: { x: number; y: number }, c: { x: number; y: number }) {
  if (kind === "rect" || kind === "freetext" || kind === "stamp" || kind === "region") {
    const r = el as SVGRectElement;
    const x = Math.min(s.x, c.x), y = Math.min(s.y, c.y);
    const w = Math.abs(c.x - s.x), h = Math.abs(c.y - s.y);
    r.setAttribute("x", `${x}`); r.setAttribute("y", `${y}`);
    r.setAttribute("width", `${w}`); r.setAttribute("height", `${h}`);
  } else if (kind === "circle") {
    const e = el as SVGEllipseElement;
    const cx = (s.x + c.x) / 2, cy = (s.y + c.y) / 2;
    const rx = Math.abs(c.x - s.x) / 2, ry = Math.abs(c.y - s.y) / 2;
    e.setAttribute("cx", `${cx}`); e.setAttribute("cy", `${cy}`);
    e.setAttribute("rx", `${rx}`); e.setAttribute("ry", `${ry}`);
  } else if (kind === "line") {
    const ln = el as SVGLineElement;
    ln.setAttribute("x1", `${s.x}`); ln.setAttribute("y1", `${s.y}`);
    ln.setAttribute("x2", `${c.x}`); ln.setAttribute("y2", `${c.y}`);
  } else if (kind === "arrow") {
    const g = el as SVGGElement;
    const ln = g.firstElementChild as SVGLineElement;
    ln.setAttribute("x1", `${s.x}`); ln.setAttribute("y1", `${s.y}`);
    ln.setAttribute("x2", `${c.x}`); ln.setAttribute("y2", `${c.y}`);
    const head = g.lastElementChild as SVGPolygonElement;
    const dx = c.x - s.x, dy = c.y - s.y;
    const len = Math.hypot(dx, dy) || 1;
    const ux = dx / len, uy = dy / len;
    const size = 10;
    const tipX = c.x, tipY = c.y;
    const baseX = c.x - ux * size, baseY = c.y - uy * size;
    const px = -uy * size * 0.5, py = ux * size * 0.5;
    head.setAttribute(
      "points",
      `${tipX},${tipY} ${baseX + px},${baseY + py} ${baseX - px},${baseY - py}`,
    );
  }
  // ink updates the polyline elsewhere via inkPoints
  void s; void c;
}

function updateInkPreview(el: SVGElement, points: { x: number; y: number }[]) {
  const poly = el as SVGPolylineElement;
  poly.setAttribute("points", points.map((p) => `${p.x},${p.y}`).join(" "));
}

function imagePxToPagePt(slot: Slot, pageIdx: number, x: number, y: number): { x: number; y: number } | null {
  const sz = pageSizes[pageIdx];
  if (!sz) return null;
  const bb = slot.wrap.getBoundingClientRect();
  const imgW = bb.width, imgH = bb.height;
  if (imgW <= 0 || imgH <= 0) return null;
  const rot = effectiveRotation(pageIdx);
  const dw = rot === 1 || rot === 3 ? sz.height_pt : sz.width_pt;
  const dh = rot === 1 || rot === 3 ? sz.width_pt : sz.height_pt;
  const sx = imgW / dw, sy = imgH / dh;
  const nx = x / sx, ny = y / sy;
  const w0 = sz.width_pt, h0 = sz.height_pt;
  switch (rot) {
    case 0: return { x: nx, y: ny };
    case 1: return { x: ny, y: h0 - nx };
    case 2: return { x: w0 - nx, y: h0 - ny };
    case 3: return { x: w0 - ny, y: nx };
    default: return { x: nx, y: ny };
  }
}

function findSlotFromTarget(target: EventTarget | null): { slot: Slot; idx: number } | null {
  let node: HTMLElement | null = target as HTMLElement | null;
  while (node && node !== document.body) {
    if (node.classList && node.classList.contains("pageSlot")) {
      const idx = parseInt(node.dataset.page || "-1", 10);
      const s = slots[idx];
      if (s) return { slot: s, idx };
      return null;
    }
    node = node.parentElement;
  }
  return null;
}

document.addEventListener("pointerdown", (ev) => {
  if (!shapeTool || !currentDoc) return;
  if (ev.button !== 0) return;
  const hit = findSlotFromTarget(ev.target);
  if (!hit) return;
  ev.preventDefault();
  ensureDrawLayerViewBox(hit.slot);
  const start = imagePxFromEvent(hit.slot, ev);
  const preview = makePreviewElement(shapeTool, annotColorInput.value);
  hit.slot.drawLayer.appendChild(preview);
  drawState = {
    pageIdx: hit.idx,
    slot: hit.slot,
    startPx: start,
    currentPx: start,
    preview,
    pointerId: ev.pointerId,
  };
  if (shapeTool === "ink") {
    drawState.inkPoints = [start];
    updateInkPreview(preview, drawState.inkPoints);
  } else {
    updatePreview(shapeTool, preview, start, start);
  }
  hit.slot.drawLayer.setPointerCapture(ev.pointerId);
});

document.addEventListener("pointermove", (ev) => {
  if (!drawState || !shapeTool) return;
  if (ev.pointerId !== drawState.pointerId) return;
  const cur = imagePxFromEvent(drawState.slot, ev);
  drawState.currentPx = cur;
  if (shapeTool === "ink" && drawState.inkPoints) {
    const last = drawState.inkPoints[drawState.inkPoints.length - 1];
    if (Math.hypot(cur.x - last.x, cur.y - last.y) >= 1.0) {
      drawState.inkPoints.push(cur);
      updateInkPreview(drawState.preview, drawState.inkPoints);
    }
  } else {
    updatePreview(shapeTool, drawState.preview, drawState.startPx, cur);
  }
});

document.addEventListener("pointerup", async (ev) => {
  if (!drawState || !shapeTool || !currentDoc) return;
  if (ev.pointerId !== drawState.pointerId) return;
  const ds = drawState;
  drawState = null;
  const tool = shapeTool;
  try {
    ds.slot.drawLayer.releasePointerCapture(ds.pointerId);
  } catch { /* ignore */ }
  ds.preview.remove();

  const sx = ds.startPx, cx = ds.currentPx;
  const dxPx = Math.abs(cx.x - sx.x), dyPx = Math.abs(cx.y - sx.y);

  if (tool === "ink") {
    const pts = ds.inkPoints || [];
    if (pts.length < 2) return;
    const stroke: number[] = [];
    for (const p of pts) {
      const pt = imagePxToPagePt(ds.slot, ds.pageIdx, p.x, p.y);
      if (!pt) continue;
      stroke.push(pt.x, pt.y);
    }
    if (stroke.length < 4) return;
    setStatus("Adding ink…");
    try {
      const snap = await invoke<DocSnapshot>("add_ink", {
        document: currentDoc,
        args: {
          page: ds.pageIdx,
          strokes: [stroke],
          color: hexToRgb01(annotColorInput.value),
          width: 1.6,
          opacity: 1.0,
          print: printAnnotations,
        },
      });
      applyDocSnapshot(snap);
      setStatus("Ink added");
    } catch (e) {
      setStatus(`ink error: ${e}`);
    }
    return;
  }

  if (dxPx < 3 && dyPx < 3 && tool !== "freetext" && tool !== "note") return;

  const start = imagePxToPagePt(ds.slot, ds.pageIdx, sx.x, sx.y);
  const end = imagePxToPagePt(ds.slot, ds.pageIdx, cx.x, cx.y);
  if (!start || !end) return;

  if (tool === "note") {
    spawnTextNoteEditor(ds.slot, ds.pageIdx, sx, start);
    return;
  }

  if (tool === "region") {
    const rect: RectPt = {
      x0: Math.min(start.x, end.x),
      y0: Math.min(start.y, end.y),
      x1: Math.max(start.x, end.x),
      y1: Math.max(start.y, end.y),
    };
    if (rect.x1 - rect.x0 < 1 || rect.y1 - rect.y0 < 1) return;
    const base = currentDocPath
      ? currentDocPath.replace(/\\/g, "/").split("/").pop()!.replace(/\.pdf$/i, "")
      : "document";
    const defaultName = `${base}_page_${ds.pageIdx + 1}_region.pdf`;
    const path = await save({
      title: "Save region as…",
      defaultPath: defaultName,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (!path || typeof path !== "string") return;
    setStatus("Saving region…");
    try {
      await invoke("save_region", {
        document: currentDoc,
        page: ds.pageIdx,
        rect,
        path,
      });
      setStatus(`Region saved → ${path}`);
    } catch (e) {
      setStatus(`Region error: ${e}`);
    }
    return;
  }

  if (tool === "freetext") {
    let rectPx = {
      x: Math.min(sx.x, cx.x),
      y: Math.min(sx.y, cx.y),
      w: Math.abs(cx.x - sx.x),
      h: Math.abs(cx.y - sx.y),
    };
    if (rectPx.w < 60 || rectPx.h < 24) {
      rectPx = { x: sx.x, y: sx.y, w: 220, h: 40 };
    }
    spawnFreeTextEditor(ds.slot, ds.pageIdx, rectPx);
    return;
  }

  if (tool === "stamp") {
    if (!stampImagePath) {
      setStatus("Pick a stamp image first");
      return;
    }
    const rect: RectPt = {
      x0: Math.min(start.x, end.x),
      y0: Math.min(start.y, end.y),
      x1: Math.max(start.x, end.x),
      y1: Math.max(start.y, end.y),
    };
    setStatus("Stamping…");
    try {
      const snap = await invoke<DocSnapshot>("add_stamp", {
        document: currentDoc,
        args: { page: ds.pageIdx, rect, image_path: stampImagePath, opacity: 1.0, print: printAnnotations },
      });
      applyDocSnapshot(snap);
      setStatus("Stamp added");
    } catch (e) {
      setStatus(`stamp error: ${e}`);
    }
    return;
  }

  // rect / circle / line / arrow
  let rect: RectPt;
  if (tool === "line" || tool === "arrow") {
    rect = { x0: start.x, y0: start.y, x1: end.x, y1: end.y };
  } else {
    rect = {
      x0: Math.min(start.x, end.x),
      y0: Math.min(start.y, end.y),
      x1: Math.max(start.x, end.x),
      y1: Math.max(start.y, end.y),
    };
  }
  const stroke = hexToRgb01(annotColorInput.value);
  setStatus(`Adding ${tool}…`);
  try {
    const snap = await invoke<DocSnapshot>("add_shape", {
      document: currentDoc,
      args: {
        page: ds.pageIdx,
        kind: tool,
        rect,
        stroke,
        border_width: 1.5,
        opacity: 1.0,
        print: printAnnotations,
      },
    });
    applyDocSnapshot(snap);
    setStatus(`${tool} added`);
  } catch (e) {
    setStatus(`shape error: ${e}`);
  }
});

function spawnFreeTextEditor(
  slot: Slot,
  pageIdx: number,
  rectPx: { x: number; y: number; w: number; h: number },
) {
  const ed = document.createElement("textarea");
  ed.style.position = "absolute";
  ed.style.left = `${rectPx.x}px`;
  ed.style.top = `${rectPx.y}px`;
  ed.style.width = `${rectPx.w}px`;
  ed.style.height = `${rectPx.h}px`;
  ed.style.border = "1px dashed #4a8cff";
  ed.style.background = "rgba(255,255,255,0.95)";
  ed.style.color = "#000";
  ed.style.font = "14px Helvetica, Arial, sans-serif";
  ed.style.padding = "2px 4px";
  ed.style.boxSizing = "border-box";
  ed.style.resize = "both";
  ed.style.zIndex = "20";
  slot.wrap.appendChild(ed);
  ed.focus();

  let committed = false;
  const cleanup = () => { ed.remove(); };
  const commit = async () => {
    if (committed) return;
    committed = true;
    const text = ed.value.trim();
    cleanup();
    if (!text || !currentDoc) return;
    const r = ed.getBoundingClientRect();
    const wrapBb = slot.wrap.getBoundingClientRect();
    const left = r.left - wrapBb.left;
    const top = r.top - wrapBb.top;
    const right = left + r.width;
    const bottom = top + r.height;
    const tl = imagePxToPagePt(slot, pageIdx, left, top);
    const br = imagePxToPagePt(slot, pageIdx, right, bottom);
    if (!tl || !br) return;
    const rect: RectPt = {
      x0: Math.min(tl.x, br.x),
      y0: Math.min(tl.y, br.y),
      x1: Math.max(tl.x, br.x),
      y1: Math.max(tl.y, br.y),
    };
    setStatus("Adding text…");
    try {
      const snap = await invoke<DocSnapshot>("add_freetext", {
        document: currentDoc,
        args: {
          page: pageIdx,
          rect,
          text,
          color: hexToRgb01(annotColorInput.value === "#ffe600" ? "#000000" : annotColorInput.value),
          font_size: 14,
          opacity: 1.0,
          print: printAnnotations,
        },
      });
      applyDocSnapshot(snap);
      setStatus("Text added");
    } catch (e) {
      setStatus(`text error: ${e}`);
    }
  };
  ed.addEventListener("blur", () => { void commit(); });
  ed.addEventListener("keydown", (e) => {
    if (e.key === "Escape") { committed = true; cleanup(); }
    else if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); ed.blur(); }
  });
}

function spawnTextNoteEditor(
  slot: Slot,
  pageIdx: number,
  anchorPx: { x: number; y: number },
  anchorPt: { x: number; y: number },
) {
  const wrap = document.createElement("div");
  wrap.style.position = "absolute";
  wrap.style.left = `${anchorPx.x}px`;
  wrap.style.top = `${anchorPx.y}px`;
  wrap.style.width = "220px";
  wrap.style.zIndex = "20";
  wrap.style.background = "#fff8b0";
  wrap.style.color = "#000";
  wrap.style.border = "1px solid #b39a00";
  wrap.style.boxShadow = "2px 2px 6px rgba(0,0,0,0.35)";
  wrap.style.font = "13px Helvetica, Arial, sans-serif";

  const head = document.createElement("div");
  head.textContent = "📝 Sticky note";
  head.style.padding = "2px 6px";
  head.style.background = "#e6cf00";
  head.style.borderBottom = "1px solid #b39a00";
  head.style.fontSize = "12px";
  wrap.appendChild(head);

  const ed = document.createElement("textarea");
  ed.placeholder = "Type comment, Enter to save, Esc to cancel";
  ed.style.display = "block";
  ed.style.width = "100%";
  ed.style.height = "80px";
  ed.style.boxSizing = "border-box";
  ed.style.border = "0";
  ed.style.background = "transparent";
  ed.style.color = "#000";
  ed.style.font = "inherit";
  ed.style.padding = "4px 6px";
  ed.style.resize = "vertical";
  wrap.appendChild(ed);

  slot.wrap.appendChild(wrap);
  ed.focus();

  let committed = false;
  const cleanup = () => { wrap.remove(); };
  const commit = async () => {
    if (committed) return;
    committed = true;
    const text = ed.value.trim();
    cleanup();
    if (!text || !currentDoc) return;
    setStatus("Adding note…");
    try {
      const snap = await invoke<DocSnapshot>("add_text_note", {
        document: currentDoc,
        args: {
          page: pageIdx,
          x_pt: anchorPt.x,
          y_pt: anchorPt.y,
          contents: text,
          color: hexToRgb01(annotColorInput.value),
          print: printAnnotations,
        },
      });
      applyDocSnapshot(snap);
      setStatus("Note added");
    } catch (e) {
      setStatus(`note error: ${e}`);
    }
  };
  ed.addEventListener("blur", () => { void commit(); });
  ed.addEventListener("keydown", (e) => {
    if (e.key === "Escape") { committed = true; cleanup(); }
    else if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); ed.blur(); }
  });
}

// ----------------- Comment panel -----------------

interface AnnotationInfo {
  id: string;
  page: number;
  subtype: string;
  rect_pt: RectPt;
  color: [number, number, number] | null;
  opacity: number;
  print: boolean;
  contents: string | null;
  author: string | null;
  modified: string | null;
  icon: string | null;
}

const commentPanelEl = document.getElementById("commentPanel") as HTMLElement;
const commentListEl = document.getElementById("commentList") as HTMLDivElement;
const commentKindFilter = document.getElementById("commentKindFilter") as HTMLSelectElement;
const commentAuthorFilter = document.getElementById("commentAuthorFilter") as HTMLInputElement;
const commentHasContent = document.getElementById("commentHasContent") as HTMLInputElement;
let cachedAnnotations: AnnotationInfo[] = [];

const SUBTYPE_ICON: Record<string, string> = {
  Highlight: "🖍",
  Underline: "U̲",
  StrikeOut: "S̶",
  Squiggly: "〰",
  Square: "▭",
  Circle: "◯",
  Line: "／",
  Ink: "✎",
  FreeText: "T",
  Stamp: "🔖",
  Text: "📝",
};

function formatPdfDate(s: string | null): string {
  if (!s) return "";
  const m = /^D:(\d{4})(\d{2})(\d{2})(\d{2})(\d{2})(\d{2})/.exec(s);
  if (!m) return s;
  return `${m[1]}-${m[2]}-${m[3]} ${m[4]}:${m[5]}`;
}

function renderComments() {
  const kindF = commentKindFilter.value;
  const authorF = commentAuthorFilter.value.trim().toLowerCase();
  const requireBody = commentHasContent.checked;
  const filtered = cachedAnnotations.filter((a) => {
    if (kindF && a.subtype !== kindF) return false;
    if (authorF && !(a.author || "").toLowerCase().includes(authorF)) return false;
    if (requireBody && !(a.contents && a.contents.trim())) return false;
    return true;
  });
  if (filtered.length === 0) {
    commentListEl.innerHTML = `<div style="color:#777;padding:0.5rem;font-size:0.85rem;">No annotations${cachedAnnotations.length > 0 ? " match the filters" : ""}.</div>`;
    return;
  }
  const byPage = new Map<number, AnnotationInfo[]>();
  for (const a of filtered) {
    const list = byPage.get(a.page) ?? [];
    list.push(a);
    byPage.set(a.page, list);
  }
  const pages = Array.from(byPage.keys()).sort((a, b) => a - b);
  const out: string[] = [];
  for (const p of pages) {
    out.push(`<div class="commentGroup"><div class="commentGroupHead">Page ${p + 1}</div>`);
    for (const a of byPage.get(p)!) {
      const icon = SUBTYPE_ICON[a.subtype] ?? "•";
      const author = a.author ? `<span class="author">${escapeHtml(a.author)}</span>` : "";
      const date = a.modified ? `<span class="date">${escapeHtml(formatPdfDate(a.modified))}</span>` : "";
      const noprint = a.print ? "" : `<span class="nopaint">no-print</span>`;
      const body = a.contents ? `<div class="body">${escapeHtml(a.contents)}</div>` : "";
      out.push(
        `<div class="commentItem" data-page="${p}" data-id="${escapeHtml(a.id)}">` +
          `<div class="row1"><span class="kind">${icon} ${a.subtype}</span>${author}${noprint}${date}</div>` +
          body +
        `</div>`,
      );
    }
    out.push(`</div>`);
  }
  commentListEl.innerHTML = out.join("");
  commentListEl.querySelectorAll<HTMLDivElement>(".commentItem").forEach((el) => {
    el.addEventListener("click", () => {
      const p = parseInt(el.dataset.page || "0", 10);
      navigateTo(p, { recordHistory: true });
    });
  });
}

function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "\"": "&quot;", "'": "&#39;" })[c]!);
}

async function refreshComments() {
  if (!currentDoc) {
    cachedAnnotations = [];
    renderComments();
    return;
  }
  try {
    cachedAnnotations = await invoke<AnnotationInfo[]>("list_annotations", { document: currentDoc });
  } catch (e) {
    setStatus(`comments error: ${e}`);
    cachedAnnotations = [];
  }
  renderComments();
}

function toggleCommentPanel() {
  const becomingVisible = commentPanelEl.classList.contains("hidden");
  commentPanelEl.classList.toggle("hidden");
  if (becomingVisible) void refreshComments();
}

notifyAnnotationsChanged = async () => {
  if (!commentPanelEl.classList.contains("hidden")) await refreshComments();
};

document.getElementById("commentToggleBtn")!.addEventListener("click", toggleCommentPanel);
document.getElementById("commentCloseBtn")!.addEventListener("click", () => commentPanelEl.classList.add("hidden"));
document.getElementById("commentReloadBtn")!.addEventListener("click", () => void refreshComments());
commentKindFilter.addEventListener("change", renderComments);
commentAuthorFilter.addEventListener("input", renderComments);
commentHasContent.addEventListener("change", renderComments);

document.getElementById("commentExportBtn")!.addEventListener("click", async () => {
  if (!currentDoc) return;
  const base = currentDocPath
    ? currentDocPath.replace(/\\/g, "/").split("/").pop()!.replace(/\.pdf$/i, "")
    : "annotations";
  const path = await save({
    title: "Export annotations as XFDF",
    defaultPath: `${base}.xfdf`,
    filters: [{ name: "XFDF", extensions: ["xfdf", "xml"] }],
  });
  if (!path || typeof path !== "string") return;
  setStatus("Exporting XFDF…");
  try {
    const len = await invoke<number>("export_annotations_xfdf", { document: currentDoc, path });
    setStatus(`Exported ${len} bytes → ${path}`);
  } catch (e) {
    setStatus(`xfdf export error: ${e}`);
  }
});

document.getElementById("commentImportBtn")!.addEventListener("click", async () => {
  if (!currentDoc) return;
  const picked = await open({
    multiple: false,
    directory: false,
    filters: [{ name: "XFDF", extensions: ["xfdf", "xml"] }],
    title: "Import XFDF",
  });
  if (!picked || typeof picked !== "string") return;
  setStatus("Importing XFDF…");
  try {
    const snap = await invoke<DocSnapshot>("import_annotations_xfdf", {
      document: currentDoc,
      path: picked,
    });
    applyDocSnapshot(snap);
    setStatus("XFDF imported");
    void refreshComments();
  } catch (e) {
    setStatus(`xfdf import error: ${e}`);
  }
});

document.addEventListener("keydown", (e) => {
  if (e.key === "F7") {
    e.preventDefault();
    toggleCommentPanel();
  }
});

// ----------------- Form fields (AcroForm) -----------------

type FormFieldKind = "text" | "checkbox" | "radio" | "combobox" | "listbox" | "pushbutton" | "signature" | "unknown";

interface FormFieldInfo {
  id: string;
  page: number;
  rect_pt: RectPt;
  kind: FormFieldKind;
  value: string | null;
  options: string[] | null;
  export_value: string | null;
  readonly: boolean;
  required: boolean;
  multiline: boolean;
  password: boolean;
  max_len: number | null;
}

let formFields: FormFieldInfo[] = [];
const formFieldsByPage = new Map<number, FormFieldInfo[]>();

async function refreshFormFields() {
  if (!currentDoc) {
    formFields = [];
    formFieldsByPage.clear();
    renderAllForms();
    return;
  }
  try {
    formFields = await invoke<FormFieldInfo[]>("list_form_fields", { document: currentDoc });
  } catch (e) {
    setStatus(`form fields error: ${e}`);
    formFields = [];
  }
  formFieldsByPage.clear();
  for (const f of formFields) {
    const list = formFieldsByPage.get(f.page) ?? [];
    list.push(f);
    formFieldsByPage.set(f.page, list);
  }
  renderAllForms();
}

function renderAllForms() {
  for (let i = 0; i < slots.length; i++) renderFormFor(i);
}

function renderFormFor(pageIdx: number) {
  const slot = slots[pageIdx];
  if (!slot) return;
  slot.formLayer.innerHTML = "";
  const list = formFieldsByPage.get(pageIdx);
  if (!list || list.length === 0) return;
  const sz = pageSizes[pageIdx];
  if (!sz) return;
  for (const f of list) {
    const left = (f.rect_pt.x0 / sz.width_pt) * 100;
    const top = (f.rect_pt.y0 / sz.height_pt) * 100;
    const width = ((f.rect_pt.x1 - f.rect_pt.x0) / sz.width_pt) * 100;
    const height = ((f.rect_pt.y1 - f.rect_pt.y0) / sz.height_pt) * 100;
    const wrap = document.createElement("div");
    wrap.className = "formField";
    if (f.readonly) wrap.classList.add("readonly");
    if (f.required) wrap.classList.add("required");
    wrap.style.left = `${left}%`;
    wrap.style.top = `${top}%`;
    wrap.style.width = `${width}%`;
    wrap.style.height = `${height}%`;
    wrap.title = `${f.id}${f.required ? " *required" : ""}`;
    if (f.kind === "text") {
      const ed = f.multiline
        ? document.createElement("textarea")
        : document.createElement("input");
      if (!f.multiline) (ed as HTMLInputElement).type = f.password ? "password" : "text";
      ed.value = f.value ?? "";
      if (f.readonly) (ed as HTMLInputElement | HTMLTextAreaElement).readOnly = true;
      ed.addEventListener("blur", () => {
        if (f.readonly) return;
        const v = (ed as HTMLInputElement | HTMLTextAreaElement).value;
        if (v === (f.value ?? "")) return;
        void writeFormField(f.id, v);
      });
      (ed as HTMLElement).addEventListener("keydown", (e: Event) => {
        const ke = e as KeyboardEvent;
        if (ke.key === "Enter" && !f.multiline) {
          ke.preventDefault();
          (ed as HTMLInputElement).blur();
        }
        if (ke.key === "Escape") {
          (ed as HTMLInputElement | HTMLTextAreaElement).value = f.value ?? "";
          (ed as HTMLInputElement | HTMLTextAreaElement).blur();
        }
      });
      wrap.appendChild(ed);
    } else if (f.kind === "checkbox") {
      wrap.classList.add("formCheck");
      const cb = document.createElement("input");
      cb.type = "checkbox";
      cb.checked = (f.value ?? "Off") !== "Off";
      cb.disabled = f.readonly;
      cb.addEventListener("change", () => {
        if (f.readonly) return;
        void writeFormField(f.id, cb.checked ? "Yes" : "Off");
      });
      wrap.appendChild(cb);
    } else if (f.kind === "radio") {
      wrap.classList.add("formCheck");
      const rb = document.createElement("input");
      rb.type = "checkbox";
      rb.checked = (f.value ?? "Off") !== "Off";
      rb.disabled = f.readonly;
      rb.addEventListener("change", () => {
        if (f.readonly) return;
        void writeFormField(f.id, rb.checked ? "On" : "Off");
      });
      wrap.appendChild(rb);
    } else if (f.kind === "combobox" || f.kind === "listbox") {
      const sel = document.createElement("select");
      if (f.kind === "listbox") sel.size = Math.max(2, f.options?.length ?? 2);
      const blank = document.createElement("option");
      blank.value = "";
      blank.textContent = "—";
      sel.appendChild(blank);
      for (const o of f.options ?? []) {
        const op = document.createElement("option");
        op.value = o;
        op.textContent = o;
        sel.appendChild(op);
      }
      sel.value = f.value ?? "";
      if (f.readonly) sel.disabled = true;
      sel.addEventListener("change", () => {
        if (f.readonly) return;
        void writeFormField(f.id, sel.value);
      });
      wrap.appendChild(sel);
    } else if (f.kind === "signature") {
      wrap.classList.add("signature");
      wrap.textContent = "Signature";
    } else {
      // pushbutton / unknown: skip rendering interactive control.
      continue;
    }
    slot.formLayer.appendChild(wrap);
  }
}

async function writeFormField(name: string, value: string) {
  if (!currentDoc) return;
  setStatus(`Setting ${name}…`);
  try {
    const snap = await invoke<DocSnapshot>("set_form_field", {
      document: currentDoc,
      name,
      value,
    });
    applyDocSnapshot(snap);
    setStatus(`Field ${name} saved`);
  } catch (e) {
    setStatus(`form error: ${e}`);
  }
}

const _origNotifyAnnotations = notifyAnnotationsChanged;
notifyAnnotationsChanged = async () => {
  await _origNotifyAnnotations();
  await refreshFormFields();
};

updateChrome();
