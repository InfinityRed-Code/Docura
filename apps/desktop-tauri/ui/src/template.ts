import { t } from "./i18n";

export function buildHtml(): string {
  return `
<main class="root">
  <header class="ribbon">
    <div class="ribbonTabs">
      <button class="ribbonTab active" data-ribbon-tab="reader">${t("tab.reader")}</button>
      <button class="ribbonTab" data-ribbon-tab="edit">${t("tab.edit")}</button>
      <button class="ribbonTab" data-ribbon-tab="forms">${t("tab.forms")}</button>
      <button class="ribbonTab" data-ribbon-tab="sign">${t("tab.sign")}</button>
      <button class="ribbonTab" data-ribbon-tab="convert">${t("tab.convert")}</button>
      <button class="ribbonTab" data-ribbon-tab="organize">${t("tab.organize")}</button>
      <button id="settingsBtnTop" class="ribbonTab settingsTab" title="${t("tip.settings")}">⚙</button>
    </div>
    <div class="ribbonBody" data-active="reader">
      <button data-ribbon="both" id="sidebarBtn" title="${t("tip.sidebar")}">☰</button>
      <button data-ribbon="both" id="commentToggleBtn" title="${t("tip.comments")}">💬</button>
      <button data-ribbon="both" id="open" title="${t("tip.open")}">📂</button>
      <button data-ribbon="both" id="recentBtn" title="${t("tip.recent")}">🕘</button>
      <div id="recentMenu" class="menu hidden"></div>
      <span data-ribbon="both" class="sep"></span>
      <button data-ribbon="both" id="undoBtn" title="${t("tip.undo")}" disabled>↶</button>
      <button data-ribbon="both" id="redoBtn" title="${t("tip.redo")}" disabled>↷</button>
      <span data-ribbon="both" class="sep"></span>
      <button data-ribbon="both" id="prevPage" title="${t("tip.prevPage")}">◀</button>
      <input data-ribbon="both" id="pageInput" type="number" min="1" value="1" style="width:4rem" />
      <span data-ribbon="both" id="pageTotal">/ 0</span>
      <button data-ribbon="both" id="nextPage" title="${t("tip.nextPage")}">▶</button>
      <button data-ribbon="both" id="gotoBtn" title="${t("tip.goto")}">⇥</button>
      <span data-ribbon="reader" class="sep"></span>
      <button data-ribbon="reader" id="zoomOut" title="${t("tip.zoomOut")}">−</button>
      <select data-ribbon="reader" id="fitSelect" title="${t("tip.fit")}">
        <option value="custom">${t("fit.custom")}</option>
        <option value="width">${t("fit.width")}</option>
        <option value="page">${t("fit.page")}</option>
        <option value="actual">${t("fit.actual")}</option>
      </select>
      <select data-ribbon="reader" id="zoomSelect" title="${t("tip.zoom")}">
        <option value="custom" id="zoomCustomOpt">100%</option>
        <option value="0.1">10%</option>
        <option value="0.5">50%</option>
        <option value="0.8">80%</option>
        <option value="1">100%</option>
        <option value="1.2">120%</option>
        <option value="1.5">150%</option>
        <option value="2">200%</option>
      </select>
      <button data-ribbon="reader" id="zoomIn" title="${t("tip.zoomIn")}">+</button>
      <button data-ribbon="reader" id="wheelZoomBtn" title="${t("tip.wheelZoom")}">🔎</button>
      <span data-ribbon="reader" class="sep"></span>
      <select data-ribbon="reader" id="viewModeSelect" title="${t("tip.viewMode")}">
        <option value="single">${t("view.single")}</option>
        <option value="continuous">${t("view.continuous")}</option>
        <option value="double">${t("view.double")}</option>
      </select>
      <button data-ribbon="reader" id="rotLeft" title="${t("tip.rotLeft")}">↶</button>
      <button data-ribbon="reader" id="rotRight" title="${t("tip.rotRight")}">↷</button>
      <button data-ribbon="reader" id="darkToggle" title="${t("tip.invert")}">☾</button>
      <button data-ribbon="reader" id="fullscreenBtn" title="${t("tip.fullscreen")}">⛶</button>
      <button data-ribbon="reader" id="presentBtn" title="${t("tip.present")}">▶▶</button>
      <span data-ribbon="reader" class="sep"></span>
      <button data-ribbon="reader" id="searchBtn" title="${t("tip.search")}">🔍</button>
      <button data-ribbon="both" id="saveAsBtn" title="${t("tip.saveAs")}">💾</button>
      <button data-ribbon="reader" id="printBtn" title="${t("tip.print")}">🖨</button>
      <button data-ribbon="reader" id="propsBtn" title="${t("tip.props")}">ⓘ</button>
      <div data-ribbon="edit" class="ribbonGroup">
        <div class="groupBody">
          <div class="modeGroup" role="radiogroup">
            <button class="modeBtn active" data-mode="select" title="${t("tip.modeSelect")}">👆</button>
            <button class="modeBtn" data-mode="delete" title="${t("tip.modeDelete")}">🗑</button>
          </div>
          <button id="deleteSelectionBtn" class="deleteAction" disabled title="${t("tip.deleteSel")}">🗑</button>
          <button id="extractSelectionBtn" disabled title="${t("tip.extractSel")}">⬇</button>
        </div>
        <div class="groupLabel">${t("grp.modi")}</div>
      </div>
      <div data-ribbon="edit" class="ribbonGroup">
        <div class="groupBody">
          <div class="menuHost">
            <button id="rotateMenuBtn" class="menuTrigger" aria-haspopup="true" aria-expanded="false" title="${t("tip.rotatePages")}">⟳</button>
            <div id="rotateMenu" class="popover hidden" role="menu">
              <button id="rotateLeftBtn" role="menuitem" title="${t("tip.rotateCcw")}">↺</button>
              <button id="rotateRightBtn" role="menuitem" title="${t("tip.rotateCw")}">↻</button>
              <button id="rotate180Btn" role="menuitem" title="${t("tip.rotate180")}">⇅</button>
            </div>
          </div>
        </div>
        <div class="groupLabel">${t("grp.rotate")}</div>
      </div>
      <div data-ribbon="edit" class="ribbonGroup">
        <div class="groupBody">
          <div class="menuHost">
            <button id="insertMenuBtn" class="menuTrigger" aria-haspopup="true" aria-expanded="false" title="${t("tip.insertPages")}">＋</button>
            <div id="insertMenu" class="popover hidden" role="menu">
              <button id="insertBeforeBtn" role="menuitem" title="${t("tip.insertBefore")}">↥</button>
              <button id="insertAfterBtn" role="menuitem" title="${t("tip.insertAfter")}">↧</button>
              <button id="insertEndBtn" role="menuitem" title="${t("tip.insertEnd")}">⤓</button>
            </div>
          </div>
        </div>
        <div class="groupLabel">${t("grp.insert")}</div>
      </div>
      <span data-ribbon="edit" id="modeHint" class="modeHint"></span>
      <button data-ribbon="organize" id="mergePdfsBtn" title="${t("tip.merge")}">⊕</button>
      <button data-ribbon="organize" id="splitPdfBtn" title="${t("tip.split")}">✂</button>
      <div data-ribbon="edit" class="ribbonGroup">
        <div class="groupBody">
          <div class="modeGroup" role="radiogroup" id="annotToolGroup" title="${t("tip.markText")}">
            <button class="modeBtn annotTool" data-annot="highlight" title="${t("tip.highlight")}">🖍</button>
            <button class="modeBtn annotTool" data-annot="underline" title="${t("tip.underline")}"><u>U</u></button>
            <button class="modeBtn annotTool" data-annot="strikeout" title="${t("tip.strikeout")}">S̶</button>
          </div>
          <input type="color" id="annotColor" value="#ffe600" title="${t("tip.annotColor")}" style="height:1.6rem;width:1.8rem;padding:0;border:1px solid #4a4a4a;background:#3a3a3a;cursor:pointer" />
          <button class="modeBtn active" id="printToggleBtn" title="${t("tip.printToggle")}">🖨</button>
        </div>
        <div class="groupLabel">${t("grp.textDecoration")}</div>
      </div>
      <div data-ribbon="edit" class="ribbonGroup">
        <div class="groupBody">
          <div class="modeGroup" role="radiogroup" id="shapeToolGroup" title="${t("tip.drawDrag")}">
            <button class="modeBtn shapeTool" data-shape="rect" title="${t("tip.rect")}">▭</button>
            <button class="modeBtn shapeTool" data-shape="circle" title="${t("tip.circle")}">◯</button>
            <button class="modeBtn shapeTool" data-shape="line" title="${t("tip.line")}">／</button>
            <button class="modeBtn shapeTool" data-shape="arrow" title="${t("tip.arrow")}">↗</button>
            <button class="modeBtn shapeTool" data-shape="ink" title="${t("tip.ink")}">✎</button>
            <button class="modeBtn shapeTool" data-shape="stamp" title="${t("tip.stamp")}">🔖</button>
            <button class="modeBtn shapeTool" data-shape="note" title="${t("tip.note")}">📝</button>
          </div>
        </div>
        <div class="groupLabel">${t("grp.draw")}</div>
      </div>
      <div data-ribbon="edit" class="ribbonGroup">
        <div class="groupBody">
          <div class="modeGroup" role="radiogroup">
            <button class="modeBtn shapeTool" data-shape="freetext" title="${t("tip.freetext")}">T</button>
          </div>
          <input type="color" id="textFgColor" value="#000000" title="${t("tip.textColor")}" style="height:1.6rem;width:1.8rem;padding:0;border:1px solid #4a4a4a;background:#3a3a3a;cursor:pointer" />
          <input type="color" id="textBgColor" value="#ffffff" title="${t("tip.bgColor")}" style="height:1.6rem;width:1.8rem;padding:0;border:1px solid #4a4a4a;background:#3a3a3a;cursor:pointer" />
          <label class="commentChk" title="${t("tip.transparentBg")}"><input type="checkbox" id="textBgTransparent" /> ⌀</label>
          <select id="textFontFamily" title="${t("tip.fontFamily")}" style="padding:0.15rem 0.3rem;">
            <option value="Helv" selected>Helvetica</option>
          </select>
          <input id="textFontSize" type="number" min="6" max="96" step="1" value="14" title="${t("tip.fontSize")}" style="width:3.5rem;" />
        </div>
        <div class="groupLabel">${t("grp.text")}</div>
      </div>
      <div data-ribbon="edit" class="ribbonGroup">
        <div class="groupBody">
          <div class="modeGroup" role="radiogroup">
            <button class="modeBtn shapeTool" data-shape="region" title="${t("tip.regionTool")}">✂</button>
          </div>
        </div>
        <div class="groupLabel">${t("grp.region")}</div>
      </div>
      <div data-ribbon="edit" class="ribbonGroup">
        <div class="groupBody">
          <button id="watermarkBtn" title="${t("tip.watermark")}">💧</button>
          <button id="headerFooterBtn" title="${t("tip.headerFooter")}">🅷</button>
        </div>
        <div class="groupLabel">${t("grp.layout")}</div>
      </div>
      <div data-ribbon="forms" class="ribbonGroup">
        <div class="groupBody">
          <button id="formPrevBtn" title="${t("tip.formPrev")}">◀</button>
          <button id="formNextBtn" title="${t("tip.formNext")}">▶</button>
        </div>
        <div class="groupLabel">${t("grp.navigate")}</div>
      </div>
      <div data-ribbon="forms" class="ribbonGroup">
        <div class="groupBody">
          <button id="formValidateBtn" title="${t("tip.formValidate")}">✓</button>
          <button id="formResetBtn" title="${t("tip.formReset")}">↺</button>
        </div>
        <div class="groupLabel">${t("grp.fill")}</div>
      </div>
      <div data-ribbon="forms" class="ribbonGroup">
        <div class="groupBody">
          <button id="formHighlightBtn" class="active" title="${t("tip.formHighlight")}">👁</button>
        </div>
        <div class="groupLabel">${t("grp.view")}</div>
      </div>
      <span data-ribbon="forms" id="formHint" class="modeHint">${t("hint.noDoc")}</span>
      <div data-ribbon="sign" class="ribbonGroup">
        <div class="groupBody">
          <button id="signSignatureBtn" title="${t("tip.placeSignature")}">${t("btn.signature")}</button>
          <button id="signInitialsBtn" title="${t("tip.placeInitials")}">${t("btn.initials")}</button>
          <button id="signManageBtn" class="menuTrigger" title="${t("tip.manageSig")}">✎</button>
        </div>
        <div class="groupLabel">${t("grp.sign")}</div>
      </div>
      <div data-ribbon="sign" class="ribbonGroup">
        <div class="groupBody">
          <button class="signMark" data-sign="check" title="${t("tip.markCheck")}">✓</button>
          <button class="signMark" data-sign="cross" title="${t("tip.markCross")}">✗</button>
          <button class="signMark" data-sign="dot" title="${t("tip.markDot")}">●</button>
          <button class="signMark" data-sign="date" title="${t("tip.markDate")}">📅</button>
          <button class="signMark" data-sign="text" title="${t("tip.signText")}">T</button>
          <input type="color" id="signColor" value="#1a3a8f" title="${t("tip.markColor")}" style="height:1.6rem;width:1.8rem;padding:0;border:1px solid #4a4a4a;background:#3a3a3a;cursor:pointer" />
        </div>
        <div class="groupLabel">${t("grp.fill")}</div>
      </div>
      <div data-ribbon="sign" class="ribbonGroup">
        <div class="groupBody">
          <button id="dsignBtn" title="${t("tip.digitalSign")}">${t("btn.digitalSign")}</button>
          <button id="dverifyBtn" title="${t("tip.verify")}">${t("btn.verify")}</button>
        </div>
        <div class="groupLabel">${t("grp.digital")}</div>
      </div>
      <span data-ribbon="sign" id="signHint" class="modeHint">${t("hint.signPick")}</span>
      <span data-ribbon="organize" id="organizeHint" class="modeHint">${t("hint.organize")}</span>
      <div data-ribbon="convert" class="ribbonGroup">
        <div class="groupBody">
          <div class="modeGroup" role="radiogroup">
            <button class="modeBtn active" data-conv="pdfa" title="${t("tip.convPdfa")}" type="button">📄→Ⓐ</button>
          </div>
        </div>
        <div class="groupLabel">${t("grp.conversion")}</div>
      </div>
      <span data-ribbon="convert" class="modeHint">${t("hint.convert")}</span>
      <span data-ribbon="both" id="status" class="status"></span>
    </div>
  </header>
  <div class="body">
    <aside id="sidebar" class="sidebar hidden">
      <div class="tabs">
        <button class="tab active" data-tab="thumbs">${t("tab.thumbs")}</button>
        <button class="tab" data-tab="outline">${t("tab.outline")}</button>
        <button class="tab" data-tab="attachments">${t("tab.files")}</button>
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
        <strong>${t("comments.title")}</strong>
        <button id="commentReloadBtn" title="${t("tip.refresh")}">↻</button>
        <button id="commentExportBtn" title="${t("tip.exportXfdf")}">⤓</button>
        <button id="commentImportBtn" title="${t("tip.importXfdf")}">⤒</button>
        <button id="commentCloseBtn" class="propsClose" title="${t("tip.close")}">✕</button>
      </div>
      <div class="commentFilters">
        <select id="commentKindFilter">
          <option value="">${t("filter.allKinds")}</option>
          <option value="Highlight">${t("kind.Highlight")}</option>
          <option value="Underline">${t("kind.Underline")}</option>
          <option value="StrikeOut">${t("kind.StrikeOut")}</option>
          <option value="Square">${t("kind.Square")}</option>
          <option value="Circle">${t("kind.Circle")}</option>
          <option value="Line">${t("kind.Line")}</option>
          <option value="Ink">${t("kind.Ink")}</option>
          <option value="FreeText">${t("kind.FreeText")}</option>
          <option value="Stamp">${t("kind.Stamp")}</option>
          <option value="Text">${t("kind.Text")}</option>
        </select>
        <input id="commentAuthorFilter" placeholder="${t("ph.author")}" />
        <label class="commentChk"><input type="checkbox" id="commentHasContent" /> ${t("lbl.withComment")}</label>
      </div>
      <div id="commentList" class="commentList"></div>
    </aside>
    <div id="mergePane" class="mergePane hidden">
      <div class="mergeHeader">
        <strong>${t("merge.title")}</strong>
        <button id="mergeAddBtn" title="${t("tip.pickPdfs")}">＋</button>
        <button id="mergeCommitBtn" disabled title="${t("tip.buildMerged")}">✔</button>
        <button id="mergeCancelBtn" title="${t("tip.cancel")}">✕</button>
        <span class="mergeHint">${t("hint.merge")}</span>
      </div>
      <div id="mergeList" class="mergeList"></div>
    </div>
    <div id="convertPane" class="mergePane convertPane hidden">
      <div class="mergeHeader">
        <strong>${t("convert.title")}</strong>
        <button id="convertAddBtn" title="${t("tip.pickPdfs")}">＋</button>
        <select id="convertLevel" title="${t("tip.pdfaLevel")}">
          <option value="1b">PDF/A-1b</option>
          <option value="2b" selected>PDF/A-2b</option>
          <option value="3b">PDF/A-3b</option>
          <option value="4">PDF/A-4</option>
        </select>
        <button id="convertOutBtn" title="${t("tip.pickOutFolder")}">📂</button>
        <button id="convertCommitBtn" disabled title="${t("tip.convertAll")}">✔</button>
        <button id="convertCancelBtn" title="${t("tip.cancel")}">✕</button>
        <span id="convertOutLabel" class="mergeHint">${t("convert.output")}</span>
      </div>
      <div id="convertList" class="mergeList"></div>
    </div>
    <div id="searchPanel" class="searchPanel hidden">
      <input id="searchInput" placeholder="${t("ph.find")}" />
      <label><input id="searchCase" type="checkbox" /> ${t("lbl.case")}</label>
      <button id="searchPrev" title="${t("tip.prevMatch")}">◀</button>
      <button id="searchNext" title="${t("tip.nextMatch")}">▶</button>
      <span id="searchCount"></span>
      <button id="searchClose" title="${t("tip.closeEsc")}">✕</button>
    </div>
    <div id="gotoPanel" class="gotoPanel hidden">
      <label>${t("lbl.gotoPage")}</label>
      <input id="gotoInput" type="number" min="1" />
      <button id="gotoGo">${t("btn.go")}</button>
      <button id="gotoClose">✕</button>
    </div>
  </div>
  <div id="dropOverlay" class="dropOverlay hidden">${t("drop.here")}</div>
  <div id="propsOverlay" class="propsOverlay hidden">
    <div class="propsModal">
      <div class="propsHeader">
        <div class="propsTabs">
          <button class="propsTab active" data-ptab="info">${t("ptab.info")}</button>
          <button class="propsTab" data-ptab="perms">${t("ptab.perms")}</button>
        </div>
        <button id="propsClose" class="propsClose">✕</button>
      </div>
      <div id="propsInfo" class="propsPanel"></div>
      <div id="propsPerms" class="propsPanel hidden"></div>
    </div>
  </div>
  <div id="settingsOverlay" class="propsOverlay hidden">
    <div class="propsModal">
      <div class="propsHeader"><strong>${t("set.title")}</strong><button id="settingsClose" class="propsClose">✕</button></div>
      <div class="propsPanel wmPanel">
        <h3 style="margin:0 0 0.3rem 0;font-size:0.95rem;">${t("set.appearance")}</h3>
        <label>${t("set.theme")}
          <select id="themeSelect">
            <option value="light">${t("theme.light")}</option>
            <option value="dark">${t("theme.dark")}</option>
          </select>
        </label>
        <label>${t("set.language")}
          <select id="langSelect">
            <option value="de">Deutsch</option>
            <option value="en">English</option>
          </select>
        </label>
        <div style="font-size:0.78rem;color:var(--text-3);">${t("set.themeHint")}</div>
      </div>
      <div class="propsPanel wmPanel">
        <h3 style="margin:0 0 0.3rem 0;font-size:0.95rem;">${t("set.pdfaExport")}</h3>
        <div id="gsStatus" style="font-size:0.85rem;color:var(--text-2);">${t("set.gsChecking")}</div>
        <div id="gsInstall" class="hidden" style="font-size:0.85rem;">
          ${t("set.gsNotFound")}
          <a id="gsLink" href="https://ghostscript.com/releases/gsdnld.html" style="color:var(--accent);">ghostscript.com</a>.
        </div>
        <label>${t("set.defaultPdfaLevel")}
          <select id="pdfaLevel">
            <option value="off" selected>${t("pdfa.off")}</option>
            <option value="1b">PDF/A-1b (ISO 19005-1)</option>
            <option value="2b">PDF/A-2b (ISO 19005-2)</option>
            <option value="3b">PDF/A-3b (ISO 19005-3)</option>
            <option value="4">PDF/A-4 (ISO 19005-4)</option>
          </select>
        </label>
        <div style="font-size:0.78rem;color:var(--text-3);">${t("set.pdfaHint")}</div>
        <div class="wmActions"><button id="pdfaSaveBtn">${t("btn.savePdfaNow")}</button><button id="gsRecheckBtn">${t("btn.recheckGs")}</button></div>
      </div>
      <div class="propsPanel wmPanel">
        <h3 style="margin:0 0 0.3rem 0;font-size:0.95rem;">${t("set.vprinter")}</h3>
        <div id="vpStatus" style="font-size:0.85rem;color:var(--text-2);">${t("set.vpChecking")}</div>
        <div id="vpPathRow" class="hidden" style="font-size:0.8rem;color:var(--text-3);"></div>
        <div id="vpOutRow" style="font-size:0.8rem;color:var(--text-3);"></div>
        <div id="vpWarn" class="hidden" style="font-size:0.8rem;color:#d98a8a;">${t("set.vpWarn")}</div>
        <div class="wmActions">
          <button id="vpInstallBtn">${t("btn.installPrinter")}</button>
          <button id="vpUninstallBtn">${t("btn.uninstallPrinter")}</button>
          <button id="vpOpenFolderBtn">${t("btn.openOutFolder")}</button>
        </div>
        <div style="font-size:0.75rem;color:var(--text-3);">${t("set.vpHint")}</div>
      </div>
    </div>
  </div>
  <div id="wmOverlay" class="propsOverlay hidden">
    <div class="propsModal">
      <div class="propsHeader"><strong>${t("wm.title")}</strong><button id="wmClose" class="propsClose">✕</button></div>
      <div class="ribbonTabs" role="tablist">
        <button id="wmTabText" class="ribbonTab active" data-wm-tab="text">${t("wm.text")}</button>
        <button id="wmTabImage" class="ribbonTab" data-wm-tab="image">${t("wm.image")}</button>
      </div>
      <div id="wmSectionText" class="propsPanel wmPanel">
        <label>${t("lbl.text")}<input id="wmText" type="text" placeholder="DRAFT" /></label>
        <label>${t("lbl.fontSize")}<input id="wmFontSize" type="number" min="8" max="400" step="1" value="72" /></label>
        <label>${t("lbl.rotationDeg")}<input id="wmRot" type="number" min="-180" max="180" step="1" value="45" /></label>
        <label>${t("lbl.opacity")}<input id="wmOpacity" type="number" min="0.05" max="1" step="0.05" value="0.25" /></label>
        <label>${t("lbl.grayBW")}<input id="wmGray" type="number" min="0" max="1" step="0.05" value="0.5" /></label>
      </div>
      <div id="wmSectionImage" class="propsPanel wmPanel hidden">
        <label>${t("lbl.imageFile")}<span style="display:flex;gap:0.3rem;align-items:center;"><input id="iwPath" type="text" readonly placeholder="${t("ph.pickImage")}" style="flex:1;" /><button id="iwPick">${t("btn.browse")}</button></span></label>
        <label>${t("lbl.scaleFrac")}<input id="iwScale" type="number" min="0.05" max="1.5" step="0.05" value="0.4" /></label>
        <label>${t("lbl.rotationDeg")}<input id="iwRot" type="number" min="-180" max="180" step="1" value="0" /></label>
        <label>${t("lbl.opacity")}<input id="iwOpacity" type="number" min="0.05" max="1" step="0.05" value="0.5" /></label>
        <label>${t("lbl.anchor")}
          <select id="iwAnchor">
            <option value="center" selected>${t("anchor.center")}</option>
            <option value="topleft">${t("anchor.topleft")}</option>
            <option value="topright">${t("anchor.topright")}</option>
            <option value="bottomleft">${t("anchor.bottomleft")}</option>
            <option value="bottomright">${t("anchor.bottomright")}</option>
          </select>
        </label>
        <label>${t("lbl.marginPt")}<input id="iwMargin" type="number" min="0" max="400" step="1" value="36" /></label>
      </div>
      <div class="wmActions" style="padding: 0.5rem 0.8rem;"><button id="wmCommit">${t("btn.apply")}</button><button id="wmCancel">${t("tip.cancel")}</button></div>
    </div>
  </div>
  <div id="hfOverlay" class="propsOverlay hidden">
    <div class="propsModal">
      <div class="propsHeader"><strong>${t("hf.title")}</strong><button id="hfClose" class="propsClose">✕</button></div>
      <div class="propsPanel wmPanel">
        <div class="hfHint">${t("hf.tokens")} <code>{page}</code>, <code>{total}</code></div>
        <div class="hfGrid">
          <label>${t("lbl.headerLeft")}<input id="hfHL" type="text" /></label>
          <label>${t("lbl.headerCenter")}<input id="hfHC" type="text" /></label>
          <label>${t("lbl.headerRight")}<input id="hfHR" type="text" /></label>
          <label>${t("lbl.footerLeft")}<input id="hfFL" type="text" /></label>
          <label>${t("lbl.footerCenter")}<input id="hfFC" type="text" placeholder="${t("ph.pageOfTotal")}" /></label>
          <label>${t("lbl.footerRight")}<input id="hfFR" type="text" /></label>
        </div>
        <div class="hfRow">
          <label>${t("lbl.fontSize")}<input id="hfFontSize" type="number" min="6" max="48" step="1" value="10" /></label>
          <label>${t("lbl.marginPt")}<input id="hfMargin" type="number" min="0" max="200" step="1" value="24" /></label>
          <label>${t("lbl.firstPageNo")}<input id="hfFirst" type="number" min="1" step="1" value="1" /></label>
          <label>${t("lbl.gray")}<input id="hfGray" type="number" min="0" max="1" step="0.05" value="0" /></label>
        </div>
        <div class="wmActions"><button id="hfCommit">${t("btn.apply")}</button><button id="hfCancel">${t("tip.cancel")}</button></div>
      </div>
    </div>
  </div>
  <div id="sigOverlay" class="propsOverlay hidden">
    <div class="propsModal">
      <div class="propsHeader"><strong>${t("sig.create")}</strong><button id="sigClose" class="propsClose">✕</button></div>
      <div class="ribbonTabs" role="tablist">
        <button id="sigTabDraw" class="ribbonTab active" data-sig-tab="draw">${t("sig.draw")}</button>
        <button id="sigTabType" class="ribbonTab" data-sig-tab="type">${t("sig.type")}</button>
        <button id="sigTabImport" class="ribbonTab" data-sig-tab="import">${t("sig.import")}</button>
      </div>
      <div class="propsPanel wmPanel">
        <label>${t("lbl.saveAs")}
          <select id="sigTarget">
            <option value="signature">${t("opt.signature")}</option>
            <option value="initials">${t("opt.initials")}</option>
          </select>
        </label>
        <div id="sigDrawSection">
          <span class="sigHint">${t("sig.drawHint")}</span>
        </div>
        <div id="sigTypeSection" class="hidden">
          <label>${t("lbl.text")}<input id="sigTypeText" type="text" placeholder="${t("ph.yourName")}" /></label>
        </div>
        <div id="sigImportSection" class="hidden">
          <span class="sigHint"><button id="sigImportBtn">${t("btn.pickImage")}</button> &nbsp; <span id="sigImportLabel">${t("sig.importHint")}</span></span>
        </div>
        <canvas id="sigCanvas" width="520" height="160" class="sigCanvas"></canvas>
        <div class="wmActions"><button id="sigClear">${t("btn.clear")}</button><button id="sigSave">${t("btn.save")}</button><button id="sigCancel">${t("tip.cancel")}</button></div>
      </div>
    </div>
  </div>
  <div id="dsignOverlay" class="propsOverlay hidden">
    <div class="propsModal">
      <div class="propsHeader"><strong>${t("dsign.title")}</strong><button id="dsignClose" class="propsClose">✕</button></div>
      <div class="ribbonTabs" role="tablist">
        <button id="dsignTabPfx" class="ribbonTab active" data-dsign-tab="pfx">${t("dsign.pfxTab")}</button>
        <button id="dsignTabWin" class="ribbonTab" data-dsign-tab="win">${t("dsign.winTab")}</button>
      </div>
      <div class="propsPanel wmPanel">
        <div id="dsignPfxSection">
          <label>${t("lbl.certFile")}
            <span style="display:flex;gap:0.3rem;align-items:center;">
              <input id="dsignPfxPath" type="text" readonly placeholder="${t("ph.pickPfx")}" style="flex:1;" />
              <button id="dsignPfxPick">${t("btn.browse")}</button>
            </span>
          </label>
          <label>${t("lbl.password")}<input id="dsignPfxPass" type="password" placeholder="${t("ph.pfxPass")}" /></label>
        </div>
        <div id="dsignWinSection" class="hidden">
          <label>${t("lbl.certificate")}
            <select id="dsignWinCert"><option value="">${t("dsign.loading")}</option></select>
          </label>
          <span class="sigHint">${t("dsign.winHint")}</span>
        </div>
        <label>${t("lbl.reason")}<input id="dsignReason" type="text" placeholder="${t("ph.reason")}" /></label>
        <label>${t("lbl.name")}<input id="dsignName" type="text" placeholder="${t("ph.signerName")}" /></label>
        <label>${t("lbl.location")}<input id="dsignLocation" type="text" placeholder="${t("ph.optional")}" /></label>
        <label>${t("lbl.tsaUrl")}
          <input id="dsignTsa" type="text" placeholder="https://tsa.example/tsr" />
        </label>
        <div class="wmActions"><button id="dsignCommit">${t("btn.sign")}</button><button id="dsignCancel">${t("tip.cancel")}</button></div>
      </div>
    </div>
  </div>
  <div id="dverifyOverlay" class="propsOverlay hidden">
    <div class="propsModal">
      <div class="propsHeader"><strong>${t("dverify.title")}</strong><button id="dverifyClose" class="propsClose">✕</button></div>
      <div id="dverifyBody" class="propsPanel"></div>
    </div>
  </div>
</main>
`;
}

export const CSS = `
  :root {
    --accent:#2b6cdf; --accent-strong:#1a5fd0; --accent-tint:#e8f0fd;
    --bg:#ffffff; --bg-2:#f4f5f7; --bg-3:#eaecf0; --hover:#eef2f8;
    --border:#e0e3e9; --border-2:#d2d7df;
    --text:#2c2e33; --text-2:#5b616c; --text-3:#8b909b;
    --canvas:#54585d; --danger:#d23b3b;
  }
  body.theme-dark {
    --accent:#4a9eff; --accent-strong:#6cb0ff; --accent-tint:#2a3a55;
    --bg:#2b2b2c; --bg-2:#252526; --bg-3:#333335; --hover:#3a3a3c;
    --border:#3a3a3a; --border-2:#4a4a4a;
    --text:#e4e4e4; --text-2:#b0b4ba; --text-3:#888d95;
    --canvas:#2b2b2b; --danger:#e06060;
  }
  /* Dark-theme tweaks for the few light-pastel accents. */
  body.theme-dark .deleteAction { background:#3a2a2a !important; color:#e09f9f !important; border-color:#5a3a3a !important; }
  body.theme-dark #mergeCommitBtn:not(:disabled) { background:#234023; color:#9fe09f; border-color:#3d6d3d; }
  body.theme-dark .sigReport.valid .sigBadge { background:#234023; color:#bfe6bf; }
  body.theme-dark .sigReport.invalid .sigBadge { background:#4a2424; color:#f0c4c4; }
  body.theme-dark .sigReport.unsupported .sigBadge { background:#4a4424; color:#e8d79a; }
  body.theme-dark .ribbonBody input[type=color] { background:var(--bg) !important; border-color:var(--border-2) !important; }
  html, body { margin:0; padding:0; height:100%;
    font-family:"Segoe UI", system-ui, sans-serif; background:var(--bg-2); color:var(--text); }
  #app, .root { height: 100%; }
  .root { display:flex; flex-direction:column; }
  .ribbon { border-bottom: 1px solid var(--border); background:var(--bg);
            position:relative; box-shadow: 0 1px 3px rgba(20,30,50,0.06); z-index:5; }
  .ribbonTabs { display:flex; gap:0.1rem; background:var(--bg-2);
                border-bottom:1px solid var(--border); padding: 0 0.4rem; align-items:center; }
  .ribbonTab.settingsTab { margin-left:auto; padding:0.25rem 0.6rem; font-size:1rem;
                           color:var(--text-2); cursor:pointer; }
  .ribbonTab.settingsTab:hover { color:var(--text); background:var(--bg-3); }
  .ribbonTab.settingsTab.active { border-bottom:none; color:var(--accent); }
  .ribbonTab { background:transparent; color:var(--text-2); border:none; padding:0.4rem 1rem;
               cursor:pointer; font:inherit; font-size:0.85rem; border-radius:4px 4px 0 0;
               border-bottom:2px solid transparent; }
  .ribbonTab:hover { background:var(--bg-3); color:var(--text); }
  .ribbonTab.active { background:var(--bg); color:var(--accent); font-weight:600;
                      border-bottom:2px solid var(--accent); }
  .ribbonBody { padding: 0.35rem 0.6rem; display:flex; gap: 0.3rem; align-items:center;
                flex-wrap: wrap; background:var(--bg); }
  .ribbonBody button, .ribbonBody select, .ribbonBody input {
    background:var(--bg); color:var(--text); border:1px solid var(--border-2); border-radius:4px;
    padding:0.25rem 0.5rem; font: inherit;
  }
  .ribbonBody > button, .ribbonBody .groupBody > button {
    background:transparent; border:1px solid transparent;
  }
  .ribbonBody button:hover:not(:disabled) { background:var(--hover); border-color:var(--border-2); }
  .ribbonBody button:disabled { opacity:0.4; cursor:default; }
  .ribbonBody button.active { background:var(--accent-tint); color:var(--accent-strong);
                              border-color:#bcd4f6; }
  .ribbonBody input[type=color] { background:var(--bg) !important; border:1px solid var(--border-2) !important; }
  .ribbonBody input:focus, .ribbonBody select:focus { outline:2px solid var(--accent-tint); border-color:var(--accent); }
  body.wheelZoomMode .viewer { cursor: grab; }
  body.wheelZoomMode.panning .viewer { cursor: grabbing; }
  body.wheelZoomMode .selectLayer, body.wheelZoomMode .drawLayer { pointer-events: none; }
  body.wheelZoomMode .pageSlot, body.wheelZoomMode .pageSlot * { user-select: none; -webkit-user-select: none; }
  .ribbonBody[data-active="reader"] > [data-ribbon="edit"],
  .ribbonBody[data-active="reader"] > [data-ribbon="forms"],
  .ribbonBody[data-active="reader"] > [data-ribbon="sign"],
  .ribbonBody[data-active="reader"] > [data-ribbon="organize"],
  .ribbonBody[data-active="reader"] > [data-ribbon="convert"] { display:none; }
  .ribbonBody[data-active="edit"] > [data-ribbon="reader"],
  .ribbonBody[data-active="edit"] > [data-ribbon="forms"],
  .ribbonBody[data-active="edit"] > [data-ribbon="sign"],
  .ribbonBody[data-active="edit"] > [data-ribbon="organize"],
  .ribbonBody[data-active="edit"] > [data-ribbon="convert"] { display:none; }
  .ribbonBody[data-active="forms"] > [data-ribbon="reader"],
  .ribbonBody[data-active="forms"] > [data-ribbon="edit"],
  .ribbonBody[data-active="forms"] > [data-ribbon="sign"],
  .ribbonBody[data-active="forms"] > [data-ribbon="organize"],
  .ribbonBody[data-active="forms"] > [data-ribbon="convert"] { display:none; }
  .ribbonBody[data-active="sign"] > [data-ribbon="reader"],
  .ribbonBody[data-active="sign"] > [data-ribbon="edit"],
  .ribbonBody[data-active="sign"] > [data-ribbon="forms"],
  .ribbonBody[data-active="sign"] > [data-ribbon="organize"],
  .ribbonBody[data-active="sign"] > [data-ribbon="convert"] { display:none; }
  .ribbonBody[data-active="organize"] > [data-ribbon="reader"],
  .ribbonBody[data-active="organize"] > [data-ribbon="edit"],
  .ribbonBody[data-active="organize"] > [data-ribbon="forms"],
  .ribbonBody[data-active="organize"] > [data-ribbon="sign"],
  .ribbonBody[data-active="organize"] > [data-ribbon="convert"] { display:none; }
  .ribbonBody[data-active="convert"] > [data-ribbon="reader"],
  .ribbonBody[data-active="convert"] > [data-ribbon="edit"],
  .ribbonBody[data-active="convert"] > [data-ribbon="forms"],
  .ribbonBody[data-active="convert"] > [data-ribbon="sign"],
  .ribbonBody[data-active="convert"] > [data-ribbon="organize"],
  .ribbonBody[data-active="convert"] > [data-ribbon="both"] { display:none; }
  .ribbonLabel { color:var(--text-3); font-size:0.85rem; margin-left:0.2rem; }
  .ribbonGroup { display:inline-flex; flex-direction:column; align-items:stretch; align-self:stretch;
                 padding: 0 0.55rem; border-right:1px solid var(--border); gap: 0.15rem; }
  .ribbonGroup:last-child { border-right:none; }
  .ribbonGroup .groupBody { display:flex; align-items:center; gap:0.25rem; flex:1; }
  .ribbonGroup .groupLabel { text-align:center; font-size:0.64rem; color:var(--text-3);
                             text-transform:uppercase; letter-spacing:0.04em; padding-top:0.1rem; }
  .modeGroup { display:inline-flex; gap: 0; border:1px solid var(--border-2); border-radius:5px;
               overflow:hidden; }
  .modeGroup .modeBtn { background:var(--bg); color:var(--text); border:none; border-right:1px solid var(--border);
                        padding:0.25rem 0.6rem; cursor:pointer; font:inherit; font-size:0.88rem; }
  .modeGroup .modeBtn:last-child { border-right:none; }
  .modeGroup .modeBtn:hover { background:var(--hover); }
  .modeGroup .modeBtn.active { background:var(--accent); color:#fff; }
  .modeGroup .modeBtn.active.danger { background:var(--danger); }
  .modeHint { color:var(--text-2); font-size:0.82rem; margin-left:0.3rem; }
  body.edit-delete .modeHint { color:var(--danger); font-weight:500; }
  .menuHost { position:relative; display:inline-block; }
  .popover { position:absolute; top:calc(100% + 4px); left:0; background:var(--bg);
             border:1px solid var(--border-2); border-radius:6px; box-shadow:0 6px 20px rgba(20,30,50,0.18);
             padding:0.25rem; display:flex; flex-direction:column; min-width:120px; z-index:100; }
  .popover.hidden { display:none; }
  .popover button { background:transparent; color:var(--text); border:none; padding:0.4rem 0.6rem;
                    text-align:left; border-radius:4px; cursor:pointer; font-size:0.9rem; }
  .popover button:hover { background:var(--hover); }
  .menuTrigger[aria-expanded="true"] { background:var(--bg-3); }
  body.edit-delete .thumb { outline:1px dashed transparent; cursor: pointer; }
  body.edit-delete .thumb:hover { outline:2px solid var(--danger); }
  .thumb.selected { outline:2px solid var(--accent); box-shadow: 0 0 0 3px rgba(43,108,223,0.3); }
  body.edit-delete .thumb.selected { outline:2px solid var(--danger); box-shadow: 0 0 0 3px rgba(192,57,43,0.35); }
  .deleteAction { background:#fbecec !important; color:#c0392b !important; border:1px solid #f0c4c4 !important; }
  .deleteAction:not(:disabled):hover { background:#c0392b !important; color:#fff !important; border-color:#c0392b !important; }
  .deleteAction:disabled { opacity:0.35; }
  .sep { display:inline-block; width:1px; height:20px; background:var(--border-2); margin: 0 0.2rem; }
  .status { margin-left:auto; color:var(--text-2); font-variant-numeric: tabular-nums; font-size: 0.85rem; }
  .body { flex:1; display:flex; min-height: 0; position:relative; }
  .sidebar { width: 200px; background:var(--bg-2); border-right:1px solid var(--border); display:flex; flex-direction:column; min-height:0; }
  .sidebar.hidden { display:none; }
  .commentPanel { width: 300px; background:var(--bg-2); border-left:1px solid var(--border); display:flex; flex-direction:column; min-height:0; }
  .commentPanel.hidden { display:none; }
  .commentHeader { display:flex; gap:0.3rem; align-items:center; padding:0.4rem 0.5rem; border-bottom:1px solid var(--border); }
  .commentHeader strong { flex:1; }
  .commentHeader button { background:var(--bg); color:var(--text); border:1px solid var(--border-2); border-radius:4px; padding:0.15rem 0.4rem; cursor:pointer; font-size:0.85rem; }
  .commentHeader button:hover { background:var(--hover); }
  .commentFilters { display:flex; flex-direction:column; gap:0.25rem; padding:0.35rem 0.5rem; border-bottom:1px solid var(--border); font-size:0.8rem; }
  .commentFilters select, .commentFilters input[type=text] { background:var(--bg); color:var(--text); border:1px solid var(--border-2); border-radius:4px; padding:0.15rem 0.3rem; font-size:0.8rem; }
  .commentChk { display:flex; align-items:center; gap:0.25rem; color:var(--text-2); }
  .commentList { flex:1; overflow:auto; padding:0.3rem; }
  .commentGroup { margin-bottom:0.5rem; }
  .commentGroupHead { font-size:0.78rem; color:var(--text-3); padding:0.2rem 0.3rem; text-transform:uppercase; letter-spacing:0.5px; }
  .commentItem { padding:0.4rem 0.5rem; border:1px solid var(--border); border-radius:5px; margin-bottom:0.3rem; cursor:pointer; background:var(--bg); }
  .commentItem:hover { background:var(--hover); border-color:var(--border-2); }
  .commentItem .row1 { display:flex; gap:0.3rem; align-items:center; font-size:0.78rem; color:var(--text-2); }
  .commentItem .row1 .kind { font-weight:bold; color:var(--text); }
  .commentItem .row1 .author { color:var(--accent); }
  .commentItem .row1 .date { margin-left:auto; color:var(--text-3); font-size:0.72rem; }
  .commentItem .body { font-size:0.85rem; color:var(--text); margin-top:0.2rem; white-space:pre-wrap; word-break:break-word; }
  .commentItem .nopaint { color:#c0392b; font-size:0.7rem; }
  .tabs { display:flex; border-bottom:1px solid var(--border); }
  .tab { flex:1; padding:0.45rem; background:var(--bg-2); color:var(--text-2); border:none; cursor:pointer; border-bottom:2px solid transparent; }
  .tab:hover { background:var(--bg-3); }
  .tab.active { background:var(--bg); color:var(--accent); font-weight:600; border-bottom: 2px solid var(--accent); }
  .panel { flex:1; overflow:auto; padding:0.4rem; }
  .panel.hidden { display:none; }
  .thumb { position:relative; display:block; margin:0 auto 0.6rem; cursor:pointer;
           border:2px solid transparent; max-width:100%; width:fit-content;
           user-select:none; -webkit-user-select:none; }
  .thumb.active { border-color:var(--accent); }
  .thumb.selected.active { border-color:transparent; }
  .thumbImg { display:block; background:white; max-width:100%;
              box-shadow:0 1px 4px rgba(20,30,50,0.25); -webkit-user-drag:none; }
  .thumbLabel { position:absolute; top:4px; right:4px;
                background:rgba(20,30,50,0.7); color:#fff; font-size:0.72rem;
                padding:1px 6px; border-radius:3px; pointer-events:none;
                font-variant-numeric: tabular-nums; }
  .outlineItem { padding: 0.15rem 0.3rem; cursor:pointer; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; font-size:0.85rem; }
  .outlineItem:hover { background:var(--hover); }
  .outlineChildren { margin-left: 0.8rem; border-left:1px solid var(--border); padding-left:0.3rem; }
  .viewer { flex:1; overflow:auto; background:var(--canvas); padding:1rem; }
  body.wheelZoomMode .viewer { padding:0; }
  .pageList { display:flex; flex-direction:column; align-items:center; gap: 1rem; box-sizing:border-box; }
  .pageList.double { flex-flow: row wrap; justify-content:center; align-content:flex-start; gap: 0.6rem 0.6rem; }
  .pageList.single > .pageSlot:not(.current) { display:none; }
  .pageSlot { position:relative; background:white; box-shadow: 0 2px 10px rgba(0,0,0,0.35); flex: 0 0 auto; }
  .pageSlot img.page { display:block; width:100%; height:100%; }
  .pageSlot .placeholder { position:absolute; inset:0; display:flex; align-items:center; justify-content:center;
                           color:#888; font-size:0.8rem; background: repeating-linear-gradient(45deg,#f0f0f0,#f0f0f0 10px,#e6e6e6 10px,#e6e6e6 20px); }
  .selectLayer { position:absolute; inset:0; cursor:text; color:transparent; user-select:text;
                 pointer-events:auto; line-height: 1; }
  .selectLayer span { position:absolute; white-space:pre; color:transparent; user-select:text;
                      -webkit-user-select:text; transform-origin: 0 0; }
  .selectLayer span::selection { background: rgba(43,108,223,0.4); color:transparent; }
  .highlightLayer { position:absolute; inset:0; pointer-events:none; }
  .drawLayer { position:absolute; inset:0; pointer-events:none; overflow:visible; }
  body.shapeMode .drawLayer { pointer-events:auto; cursor:crosshair; }
  body.shapeMode .selectLayer { pointer-events:none; }
  body.signMode .drawLayer { pointer-events:auto; cursor:crosshair; }
  body.signMode .selectLayer { pointer-events:none; }
  .signMark.active { background:var(--accent-tint) !important; color:var(--accent-strong) !important; border-color:#bcd4f6 !important; }
  .sigCanvas { display:block; width:100%; height:160px; background:#fff;
               border:1px dashed var(--accent); border-radius:5px; cursor:crosshair; touch-action:none; }
  .sigHint { color:var(--text-3); font-size:0.82rem; margin:0.2rem 0; display:block; }
  #sigSave { background:var(--accent); border-color:var(--accent); color:#fff; }
  #sigSave:hover { background:var(--accent-strong); }
  #dsignCommit { background:var(--accent); border-color:var(--accent); color:#fff; }
  #dsignCommit:hover { background:var(--accent-strong); }
  .sigReport { border:1px solid var(--border); border-radius:6px; padding:0.5rem 0.7rem; margin-bottom:0.5rem; background:var(--bg); }
  .sigReport .sigBadge { display:inline-block; padding:0.08rem 0.55rem; border-radius:4px; font-size:0.78rem; font-weight:600; }
  .sigReport.valid { border-color:#bfe0bf; } .sigReport.valid .sigBadge { background:#e4f5e4; color:#2c7a2c; }
  .sigReport.invalid { border-color:#f0c4c4; } .sigReport.invalid .sigBadge { background:#fbe6e6; color:#c0392b; }
  .sigReport.unsupported .sigBadge { background:#fcf2d6; color:#9a7b18; }
  .sigReport .sigRow { font-size:0.83rem; color:var(--text-2); margin-top:0.25rem; }
  .sigReport .sigRow b { color:var(--text); font-weight:600; }
  .sigEmpty { color:var(--text-3); padding:0.5rem; }
  .highlightLayer .hit { position:absolute; background: rgba(255,220,0,0.45); }
  .highlightLayer .hit.current { background: rgba(255,140,0,0.6); outline:1px solid orange; }
  .linkLayer { position:absolute; inset:0; pointer-events:none; }
  .linkLayer a { position:absolute; pointer-events:auto; display:block;
                 background: transparent; border: 0; cursor:pointer;
                 text-decoration:none; }
  .linkLayer a:hover { background: rgba(43,108,223,0.18); outline: 1px solid rgba(43,108,223,0.55); }
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
                  background: #ffffff; outline:1px solid var(--accent); }
  .formLayer .formCheck { display:flex; align-items:center; justify-content:center; cursor:pointer; }
  .formLayer .formCheck input { margin:0; transform: scale(1.2); }
  .formLayer .formField.readonly { background: #e8e8e8; border-color: rgba(150,150,150,0.5); }
  .formLayer .formField.readonly input, .formLayer .formField.readonly textarea, .formLayer .formField.readonly select { background: #e8e8e8; }
  .formLayer .formField.required { border-color: rgba(220,160,40,0.85); }
  .formLayer .formField.invalid { border: 2px solid #d04848; box-shadow: 0 0 0 2px rgba(208,72,72,0.25); }
  .ribbonGroup.hidden { display:none; }
  body.formsNoHl .formLayer .formField { background: transparent; border-color: transparent; }
  body.formsNoHl .formLayer input[type=text], body.formsNoHl .formLayer input[type=password],
  body.formsNoHl .formLayer textarea, body.formsNoHl .formLayer select { background: transparent; }
  body.formsNoHl .formLayer input:focus, body.formsNoHl .formLayer textarea:focus,
  body.formsNoHl .formLayer select:focus { background:#fff; outline:1px solid var(--accent); }
  .formLayer .formField.signature { background: rgba(160,80,160,0.08); border:1px dashed rgba(160,80,160,0.55); display:flex; align-items:center; justify-content:center; color:#a05ca0; font-size:0.78rem; pointer-events:none; }
  .searchPanel, .gotoPanel { position:absolute; top: 0.5rem; right: 1rem; background:var(--bg);
                 border:1px solid var(--border-2); border-radius:6px; padding: 0.35rem;
                 box-shadow:0 6px 20px rgba(20,30,50,0.18);
                 display:flex; gap:0.3rem; align-items:center; z-index: 20; }
  .searchPanel.hidden, .gotoPanel.hidden { display:none; }
  .searchPanel input[type=text], .searchPanel #searchInput {
    background:var(--bg); color:var(--text); border:1px solid var(--border-2); border-radius:4px; padding:0.2rem 0.4rem; width:16rem;
  }
  .searchPanel button, .gotoPanel button { background:var(--bg); color:var(--text); border:1px solid var(--border-2); border-radius:4px; padding:0.2rem 0.5rem; cursor:pointer; }
  .searchPanel button:hover, .gotoPanel button:hover { background:var(--hover); }
  .gotoPanel input { background:var(--bg); color:var(--text); border:1px solid var(--border-2); border-radius:4px; padding:0.2rem 0.4rem; width:6rem; }
  .menu { position:absolute; top:2.2rem; left:0.6rem; background:var(--bg); border:1px solid var(--border-2);
          border-radius:6px; padding:0.3rem 0; min-width: 22rem; z-index:30; max-height:60vh; overflow:auto;
          box-shadow:0 8px 24px rgba(20,30,50,0.2); }
  .menu.hidden { display:none; }
  .menu .item { padding: 0.35rem 0.7rem; cursor:pointer; font-size:0.85rem; white-space:nowrap;
                overflow:hidden; text-overflow:ellipsis; color:var(--text); }
  .menu .item:hover { background:var(--hover); }
  .menu .empty { padding:0.5rem 0.7rem; color:var(--text-3); }
  .dropOverlay { position:fixed; inset:0; background: rgba(43,108,223,0.18);
                 border: 4px dashed var(--accent); display:flex; align-items:center;
                 justify-content:center; font-size: 2rem; color:var(--accent-strong); pointer-events:none; z-index:1000; }
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
  body.organize .thumb.dropBefore { box-shadow: inset 4px 0 0 var(--accent); }
  body.organize .thumb.dropAfter  { box-shadow: inset -4px 0 0 var(--accent); }
  body.organize .thumbLabel { font-size:0.78rem; padding:2px 7px; }
  .dragPreview {
    position:fixed; pointer-events:none; z-index:9999;
    background:var(--accent); color:#fff; padding:4px 10px; border-radius:999px;
    font-size:0.82rem; box-shadow:0 3px 10px rgba(20,30,50,0.35);
  }
  .mergePane { flex:1; display:none; flex-direction:column; background:var(--bg-2); overflow:hidden; }
  body.merge #mergePane { display:flex; }
  body.convert #convertPane { display:flex; }
  body.convert .viewer { display:none; }
  body.merge #viewer, body.merge #sidebar { display:none !important; }
  .mergeHeader {
    display:flex; align-items:center; gap:0.6rem;
    padding:0.6rem 1rem; border-bottom:1px solid var(--border); background:var(--bg);
  }
  .mergeHeader button {
    background:var(--bg); color:var(--text); border:1px solid var(--border-2); border-radius:4px;
    padding:0.35rem 0.7rem; cursor:pointer; font:inherit;
  }
  .mergeHeader button:hover:not(:disabled) { background:var(--hover); }
  .mergeHeader button:disabled { opacity:0.4; cursor:not-allowed; }
  #mergeCommitBtn:not(:disabled) { background:#e4f5e4; color:#2c7a2c; border-color:#bfe0bf; }
  #mergeCommitBtn:not(:disabled):hover { background:#d3eed3; }
  .mergeHint { color:var(--text-3); font-size:0.85rem; margin-left:auto; }
  .mergeList {
    flex:1; overflow:auto; padding:1rem;
    display:grid; grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap:1rem; align-content:start;
  }
  body.merge.mergeDrop .mergeList { box-shadow: inset 0 0 0 3px var(--accent); }
  .mergeTile {
    position:relative; background:var(--bg); border:1px solid var(--border-2); border-radius:8px;
    padding:0.8rem; display:flex; flex-direction:column; gap:0.3rem;
    cursor:grab; user-select:none; box-shadow:0 1px 3px rgba(20,30,50,0.08);
  }
  .mergeTile:active { cursor:grabbing; }
  .mergeTile .mergeIndex {
    position:absolute; top:4px; left:6px;
    background:var(--accent); color:#fff; font-size:0.75rem;
    padding:1px 6px; border-radius:10px; font-variant-numeric:tabular-nums;
  }
  .mergeTile .mergeName {
    font-size:0.9rem; color:var(--text); word-break:break-all;
    margin-top:1.2rem; overflow:hidden; display:-webkit-box;
    -webkit-line-clamp:3; -webkit-box-orient:vertical;
  }
  .mergeTile .mergePath { font-size:0.72rem; color:var(--text-3); word-break:break-all; }
  .mergeTile .mergeRemove {
    position:absolute; top:4px; right:6px; background:transparent; color:var(--text-3);
    border:none; cursor:pointer; font-size:1rem; padding:0 4px;
  }
  .mergeTile .mergeRemove:hover { color:var(--danger); }
  .mergeTile.dragging { opacity:0.35; }
  .mergeTile.dropBefore { box-shadow: inset 4px 0 0 var(--accent); }
  .mergeTile.dropAfter  { box-shadow: inset -4px 0 0 var(--accent); }
  .mergeEmpty { color:var(--text-3); padding:2rem; text-align:center; grid-column: 1 / -1; font-size:0.95rem; }
  .dropMark { outline:3px solid var(--accent); outline-offset:2px; }
  body.presentation .pageList { gap:0; }
  body.dark .pageSlot img.page { filter: invert(0.92) hue-rotate(180deg); }
  body.dark .pageSlot { background:#222; }
  body.dark .selectLayer span::selection { background: rgba(43,108,223,0.5); }
  .attachmentItem { display:flex; gap:0.4rem; align-items:center; padding:0.3rem; border-bottom:1px solid var(--border); font-size:0.82rem; }
  .attachmentItem .name { flex:1; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .attachmentItem .size { color:var(--text-3); font-variant-numeric: tabular-nums; }
  .attachmentItem button { background:var(--bg); color:var(--text); border:1px solid var(--border-2); border-radius:4px; padding:0.1rem 0.4rem; cursor:pointer; }
  .attachmentItem button:hover { background:var(--hover); }
  .attachEmpty { color:var(--text-3); padding:0.4rem; font-size:0.85rem; }
  .propsOverlay { position:fixed; inset:0; background: rgba(20,30,50,0.4); display:flex; align-items:center; justify-content:center; z-index:40; }
  .propsOverlay.hidden { display:none; }
  .propsModal { background:var(--bg); border:1px solid var(--border-2); border-radius:10px; min-width: 28rem; max-width:40rem; max-height:80vh; display:flex; flex-direction:column; box-shadow:0 16px 48px rgba(20,30,50,0.28); overflow:hidden; }
  .propsHeader { display:flex; align-items:center; border-bottom:1px solid var(--border); padding: 0.2rem 0.4rem; background:var(--bg-2); }
  .propsHeader strong { padding:0.3rem 0.4rem; }
  .propsTabs { display:flex; flex:1; }
  .propsTab { background:transparent; color:var(--text-2); border:none; padding:0.5rem 0.9rem; cursor:pointer; font:inherit; border-bottom:2px solid transparent; }
  .propsTab:hover { color:var(--text); }
  .propsTab.active { color:var(--accent); font-weight:600; border-bottom: 2px solid var(--accent); }
  .propsClose { background:transparent; color:var(--text-2); border:none; padding:0.3rem 0.6rem; cursor:pointer; font-size:1rem; }
  .propsClose:hover { color:var(--text); }
  .propsPanel { padding: 0.8rem 1rem; overflow:auto; }
  .propsPanel.hidden { display:none; }
  .propsRow { display:flex; gap:0.6rem; padding:0.25rem 0; font-size:0.88rem; }
  .propsRow .k { width: 10rem; color:var(--text-3); }
  .propsRow .v { flex:1; color:var(--text); word-break: break-word; }
  .propsRow .v.no { color:var(--text-3); }
  .propsRow .v.yes { color:#2c7a2c; }
  .propsRow .v.bad { color:#c0392b; }
  .wmPanel label { display:flex; flex-direction:column; gap:0.2rem; font-size:0.85rem; color:var(--text-2); margin:0.4rem 0; }
  .wmPanel input[type="text"], .wmPanel input[type="number"], .wmPanel input[type="password"], .wmPanel select {
    background:var(--bg); color:var(--text); border:1px solid var(--border-2); border-radius:5px; padding:0.35rem 0.5rem; font:inherit; }
  .wmPanel input:focus, .wmPanel select:focus { outline:2px solid var(--accent-tint); border-color:var(--accent); }
  .wmActions { display:flex; gap:0.5rem; justify-content:flex-end; margin-top:0.7rem; }
  .wmActions button { background:var(--bg); color:var(--text); border:1px solid var(--border-2); border-radius:5px; padding:0.35rem 0.9rem; cursor:pointer; }
  .wmActions button:hover { background:var(--hover); }
  #wmCommit, #hfCommit { background:var(--accent); border-color:var(--accent); color:#fff; }
  #wmCommit:hover, #hfCommit:hover { background:var(--accent-strong); }
  .hfHint { color:var(--text-3); font-size:0.82rem; margin-bottom:0.4rem; }
  .hfHint code { background:var(--bg-3); padding:0 0.25rem; border-radius:3px; color:var(--text); }
  .hfGrid { display:grid; grid-template-columns:1fr 1fr 1fr; gap:0.4rem; }
  .hfRow { display:grid; grid-template-columns:1fr 1fr 1fr 1fr; gap:0.4rem; margin-top:0.4rem; }
`;

export function applyTemplate() {
  const app = document.getElementById("app")!;
  app.innerHTML = buildHtml();
  const style = document.createElement("style");
  style.textContent = CSS;
  document.head.appendChild(style);
}
