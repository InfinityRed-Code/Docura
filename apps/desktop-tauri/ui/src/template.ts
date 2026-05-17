export const HTML = `
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
            <button class="modeBtn shapeTool" data-shape="freetext" title="Text box: drag a rectangle, then type">T</button>
          </div>
          <input type="color" id="textFgColor" value="#000000" title="Text color" style="height:1.6rem;width:1.8rem;padding:0;border:1px solid #4a4a4a;background:#3a3a3a;cursor:pointer" />
          <input type="color" id="textBgColor" value="#ffffff" title="Background color (right-click to clear for transparent)" style="height:1.6rem;width:1.8rem;padding:0;border:1px solid #4a4a4a;background:#3a3a3a;cursor:pointer" />
          <label class="commentChk" title="Transparent background"><input type="checkbox" id="textBgTransparent" /> ⌀</label>
          <select id="textFontFamily" title="Font family" style="padding:0.15rem 0.3rem;">
            <option value="Helv" selected>Helvetica</option>
          </select>
          <input id="textFontSize" type="number" min="6" max="96" step="1" value="14" title="Font size" style="width:3.5rem;" />
        </div>
        <div class="groupLabel">Text</div>
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
      <div data-ribbon="edit" id="formGroup" class="ribbonGroup hidden">
        <div class="groupBody">
          <button id="formValidateBtn" title="Check required form fields">✓</button>
          <button id="formResetBtn" title="Reset all form fields to defaults">↺</button>
        </div>
        <div class="groupLabel">Form</div>
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

export const CSS = `
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
                  background: #d6e7ff; border: 1px solid rgba(74,140,255,0.55);
                  box-sizing:border-box; }
  .formLayer .formField:hover { background: #c6dbfb; border-color: rgba(74,140,255,0.9); }
  .formLayer input[type=text], .formLayer input[type=password], .formLayer textarea, .formLayer select {
                  width:100%; height:100%; box-sizing:border-box; padding:0 2px;
                  border:0; background: #d6e7ff; color:#000; font: inherit;
                  font-family: Helvetica, Arial, sans-serif; resize:none; }
  .formLayer input[type=text]:focus, .formLayer input[type=password]:focus, .formLayer textarea:focus, .formLayer select:focus {
                  background: #ffffff; outline:1px solid #4a8cff; }
  .formLayer .formCheck { display:flex; align-items:center; justify-content:center; cursor:pointer; }
  .formLayer .formCheck input { margin:0; transform: scale(1.2); }
  .formLayer .formField.readonly { background: #e8e8e8; border-color: rgba(150,150,150,0.5); }
  .formLayer .formField.readonly input, .formLayer .formField.readonly textarea, .formLayer .formField.readonly select { background: #e8e8e8; }
  .formLayer .formField.required { border-color: rgba(220,160,40,0.85); }
  .formLayer .formField.invalid { border: 2px solid #d04848; box-shadow: 0 0 0 2px rgba(208,72,72,0.25); }
  .ribbonGroup.hidden { display:none; }
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

export function applyTemplate() {
  const app = document.getElementById("app")!;
  app.innerHTML = HTML;
  const style = document.createElement("style");
  style.textContent = CSS;
  document.head.appendChild(style);
}
