import { t } from "./i18n";
import appIcon from "./assets/logo.svg";
import logoText from "./assets/logo-text.svg";

/** Shapes of the comment tab's "Shapes" menu: [tool, lucide icon], grouped. */
export const SHAPE_MENU: { head: string; items: [string, string][] }[] = [
  { head: "shape.lines", items: [["line", "slash"], ["arrow", "move-up-right"], ["arrow2", "move-diagonal-2"], ["polyline", "spline"]] },
  { head: "shape.shapes", items: [
    ["rect", "square"], ["circle", "circle"], ["triangle", "triangle"], ["diamond", "diamond"],
    ["hexagon", "hexagon"], ["star", "star"], ["polygon", "pentagon"],
  ] },
  { head: "shape.symbols", items: [["cloud", "cloud"], ["callout", "message-square"], ["check", "check"]] },
];

function shapeMenu(): string {
  return SHAPE_MENU.map((g) =>
    `<div class="popHead">${t(g.head)}</div><div class="shapeGrid">` +
    g.items.map(([k, icon]) =>
      `<button class="mi shapeTool" data-shape="${k}" data-icon="${icon}" role="menuitem"><i data-lucide="${icon}"></i><span>${t(`tip.${k}`)}</span></button>`,
    ).join("") + `</div>`,
  ).join("");
}

export function buildHtml(): string {
  return `
<main class="root">
  <header class="ribbon">
    <div class="titleBar" data-tauri-drag-region>
      <span class="appGlyph" data-tauri-drag-region><img src="${appIcon}" alt="" draggable="false" /></span>
      <div class="qat">
        <button id="open" title="${t("tip.open")}"><i data-lucide="folder-open"></i></button>
        <button id="recentBtn" title="${t("tip.recent")}" class="hasCaret"><i data-lucide="history"></i></button>
        <div id="recentMenu" class="menu hidden"></div>
        <button id="saveBtn" data-doc title="${t("tip.save")}"><i data-lucide="save"></i></button>
        <button id="saveAsBtn" data-doc title="${t("tip.saveAs")}"><i data-lucide="save-all"></i></button>
        <button id="printBtn" data-doc title="${t("tip.print")}"><i data-lucide="printer"></i></button>
        <span class="sep"></span>
        <button id="undoBtn" title="${t("tip.undo")}" disabled><i data-lucide="undo-2"></i></button>
        <button id="redoBtn" title="${t("tip.redo")}" disabled><i data-lucide="redo-2"></i></button>
      </div>
      <div class="titleDrag" data-tauri-drag-region><span id="titlebarTitle">Docura PDF</span></div>
      <button id="settingsBtnTop" class="winBtn" title="${t("tip.settings")}"><i data-lucide="settings"></i></button>
      <div class="winControls">
        <button id="winMin" title="${t("tip.winMin")}" tabindex="-1"><i data-lucide="minus"></i></button>
        <button id="winMax" title="${t("tip.winMax")}" tabindex="-1"><i data-lucide="square"></i></button>
        <button id="winClose" title="${t("tip.winClose")}" tabindex="-1"><i data-lucide="x"></i></button>
      </div>
    </div>
    <div class="tabStrip" role="tablist">
      <button class="ribbonTab active" role="tab" data-ribbon-tab="reader">${t("tab.reader")}</button>
      <button class="ribbonTab" role="tab" data-ribbon-tab="comment">${t("tab.comment")}</button>
      <button class="ribbonTab" role="tab" data-ribbon-tab="organize">${t("tab.organize")}</button>
      <button class="ribbonTab" role="tab" data-ribbon-tab="edit">${t("tab.edit")}</button>
      <button class="ribbonTab" role="tab" data-ribbon-tab="sign">${t("tab.sign")}</button>
      <button class="ribbonTab" role="tab" data-ribbon-tab="convert">${t("tab.convert")}</button>
      <button class="ribbonTab" role="tab" data-ribbon-tab="protect">${t("tab.protect")}</button>
      <button id="ribbonPinBtn" class="ribbonPin" type="button"><i data-lucide="chevron-down"></i></button>
    </div>
    <div class="ribbonBody" data-active="reader">
      <!-- ===== Start ===== -->
      <div data-ribbon="reader" class="ribbonGroup">
        <div class="groupBody">
          <button class="cmd" id="wheelZoomBtn" data-doc title="${t("tip.wheelZoom")}"><i data-lucide="hand"></i><span>${t("lbl.pan")}</span></button>
          <button class="cmd shapeTool" data-shape="region" data-doc title="${t("tip.regionTool")}"><i data-lucide="crop"></i><span>${t("lbl.region")}</span></button>
          <button class="cmd" id="searchBtn" data-doc title="${t("tip.search")}"><i data-lucide="search"></i><span>${t("lbl.search")}</span></button>
        </div>
        <div class="groupLabel">${t("grp.tools")}</div>
      </div>
      <div data-ribbon="reader" class="ribbonGroup">
        <div class="groupBody">
          <div class="fieldCol">
            <select id="viewModeSelect" title="${t("tip.viewMode")}">
              <option value="single">${t("view.single")}</option>
              <option value="continuous">${t("view.continuous")}</option>
              <option value="double">${t("view.double")}</option>
            </select>
            <span class="fieldCaption"><i data-lucide="layout-grid"></i>${t("lbl.layout")}</span>
          </div>
          <div class="menuHost">
            <button class="cmd menuTrigger" id="viewRotateBtn" data-doc aria-haspopup="true" aria-expanded="false" title="${t("tip.rotateView")}"><i data-lucide="rotate-cw-square"></i><span>${t("lbl.rotateView")}</span></button>
            <div class="popover hidden" role="menu">
              <button class="mi" id="rotLeft" role="menuitem"><i data-lucide="rotate-ccw"></i><span>${t("tip.rotateCcw")}</span></button>
              <button class="mi" id="rotRight" role="menuitem"><i data-lucide="rotate-cw"></i><span>${t("tip.rotateCw")}</span></button>
            </div>
          </div>
          <button class="cmd" id="darkToggle" title="${t("tip.invert")}"><i data-lucide="moon"></i><span>${t("lbl.invert")}</span></button>
          <button class="cmd" id="fullscreenBtn" title="${t("tip.fullscreen")}"><i data-lucide="maximize"></i><span>${t("lbl.fullscreen")}</span></button>
          <button class="cmd" id="presentBtn" data-doc title="${t("tip.present")}"><i data-lucide="presentation"></i><span>${t("lbl.present")}</span></button>
        </div>
        <div class="groupLabel">${t("grp.display")}</div>
      </div>
      <div data-ribbon="reader" class="ribbonGroup">
        <div class="groupBody">
          <button class="cmd" id="propsBtn" data-doc title="${t("tip.props")}"><i data-lucide="info"></i><span>${t("lbl.props")}</span></button>
        </div>
        <div class="groupLabel">${t("grp.document")}</div>
      </div>

      <!-- ===== Kommentieren ===== -->
      <div data-ribbon="comment" class="ribbonGroup">
        <div class="groupBody" id="annotToolGroup">
          <button class="cmd annotTool" data-annot="highlight" data-doc title="${t("tip.highlight")}"><i data-lucide="highlighter"></i><span>${t("lbl.highlight")}</span></button>
          <button class="cmd annotTool" data-annot="underline" data-doc title="${t("tip.underline")}"><i data-lucide="underline"></i><span>${t("lbl.underline")}</span></button>
          <button class="cmd annotTool" data-annot="strikeout" data-doc title="${t("tip.strikeout")}"><i data-lucide="strikethrough"></i><span>${t("lbl.strikeout")}</span></button>
          <button class="cmd annotTool" data-annot="squiggly" data-doc title="${t("tip.squiggly")}"><i data-lucide="waves"></i><span>${t("lbl.squiggly")}</span></button>
          <label class="fieldCol" title="${t("tip.annotColor")}">
            <input type="color" id="annotColor" value="#ffe600" />
            <span class="fieldCaption"><i data-lucide="palette"></i>${t("lbl.color")}</span>
          </label>
        </div>
        <div class="groupLabel">${t("grp.markup")}</div>
      </div>
      <div data-ribbon="comment" class="ribbonGroup">
        <div class="groupBody">
          <button class="cmd shapeTool" data-shape="note" data-doc title="${t("tip.note")}"><i data-lucide="sticky-note"></i><span>${t("lbl.note")}</span></button>
          <div class="menuHost">
            <button class="cmd menuTrigger" id="shapeMenuBtn" data-doc aria-haspopup="true" aria-expanded="false" title="${t("tip.drawDrag")}"><i data-lucide="shapes"></i><span>${t("lbl.draw")}</span></button>
            <div class="popover hidden shapePop" role="menu" id="shapeToolGroup">
              ${shapeMenu()}
            </div>
          </div>
          <button class="cmd shapeTool" data-shape="ink" data-doc title="${t("tip.ink")}"><i data-lucide="pen-line"></i><span>${t("lbl.ink")}</span></button>
          <label class="fieldCol" title="${t("tip.fill")}">
            <input type="hidden" id="shapeFill" value="" data-color data-color-none="${t("cp.noFill")}" />
            <span class="fieldCaption"><i data-lucide="paint-bucket"></i>${t("lbl.fill")}</span>
          </label>
        </div>
        <div class="groupLabel">${t("grp.addComment")}</div>
      </div>
      <div data-ribbon="comment" class="ribbonGroup">
        <div class="groupBody">
          <button class="cmd" id="eraseAnnotBtn" data-doc title="${t("tip.eraseAnnot")}"><i data-lucide="eraser"></i><span>${t("lbl.erase")}</span></button>
          <button class="cmd" id="commentToggleBtn" title="${t("tip.comments")}"><i data-lucide="message-square-text"></i><span>${t("lbl.commentList")}</span></button>
          <button class="cmd active" id="printToggleBtn" title="${t("tip.printToggle")}"><i data-lucide="printer"></i><span>${t("lbl.print")}</span></button>
        </div>
        <div class="groupLabel">${t("grp.manage")}</div>
      </div>

      <!-- ===== Seiten ===== -->
      <div data-ribbon="organize" class="ribbonGroup">
        <div class="groupBody">
          <button class="cmd" id="rotateLeftBtn" data-doc title="${t("tip.rotateCcw")}"><i data-lucide="rotate-ccw"></i><span>${t("lbl.rotLeft")}</span></button>
          <button class="cmd" id="rotateRightBtn" data-doc title="${t("tip.rotateCw")}"><i data-lucide="rotate-cw"></i><span>${t("lbl.rotRight")}</span></button>
          <button class="cmd deleteAction" id="deleteSelectionBtn" disabled title="${t("tip.deleteSel")}"><i data-lucide="trash-2"></i><span>${t("lbl.delPage")}</span></button>
          <button class="cmd" id="extractSelectionBtn" disabled title="${t("tip.extractSel")}"><i data-lucide="file-output"></i><span>${t("lbl.extract")}</span></button>
        </div>
        <div class="groupLabel">${t("grp.pages")}</div>
      </div>
      <div data-ribbon="organize" class="ribbonGroup">
        <div class="groupBody">
          <div class="menuHost">
            <button id="insertMenuBtn" class="cmd menuTrigger" data-doc aria-haspopup="true" aria-expanded="false" title="${t("tip.insertPages")}"><i data-lucide="file-plus-2"></i><span>${t("lbl.insert")}</span></button>
            <div id="insertMenu" class="popover hidden" role="menu">
              <button id="insertBeforeBtn" class="mi" role="menuitem"><i data-lucide="arrow-up-to-line"></i><span>${t("tip.insertBefore")}</span></button>
              <button id="insertAfterBtn" class="mi" role="menuitem"><i data-lucide="arrow-down-to-line"></i><span>${t("tip.insertAfter")}</span></button>
              <button id="insertEndBtn" class="mi" role="menuitem"><i data-lucide="list-end"></i><span>${t("tip.insertEnd")}</span></button>
            </div>
          </div>
          <button class="cmd" id="mergePdfsBtn" title="${t("tip.merge")}"><i data-lucide="merge"></i><span>${t("lbl.merge")}</span></button>
          <button class="cmd" id="splitPdfBtn" data-doc title="${t("tip.split")}"><i data-lucide="scissors"></i><span>${t("lbl.split")}</span></button>
        </div>
        <div class="groupLabel">${t("grp.document")}</div>
      </div>
      <span data-ribbon="organize" id="organizeHint" class="modeHint">${t("hint.organize")}</span>

      <!-- ===== Bearbeiten ===== -->
      <div data-ribbon="edit" class="ribbonGroup">
        <div class="groupBody">
          <button class="cmd shapeTool" data-shape="freetext" data-doc title="${t("tip.freetext")}"><i data-lucide="type"></i><span>${t("lbl.textAdd")}</span></button>
          <div class="textFmt" id="textFmt" role="group" aria-label="${t("lbl.textFormat")}" title="${t("tip.textFormat")}"></div>
        </div>
        <div class="groupLabel">${t("grp.text")}</div>
      </div>
      <div data-ribbon="edit" class="ribbonGroup">
        <div class="groupBody">
          <button class="cmd shapeTool" data-shape="stamp" data-doc title="${t("tip.stamp")}"><i data-lucide="image-plus"></i><span>${t("lbl.addImage")}</span></button>
          <button class="cmd shapeTool" data-shape="model3d" data-doc title="${t("tip.model3d")}"><i data-lucide="rotate-3d"></i><span>${t("lbl.model3d")}</span></button>
        </div>
        <div class="groupLabel">${t("grp.image")}</div>
      </div>
      <div data-ribbon="edit" class="ribbonGroup">
        <div class="groupBody">
          <button class="cmd" id="watermarkBtn" data-doc title="${t("tip.watermark")}"><i data-lucide="droplet"></i><span>${t("lbl.watermark")}</span></button>
          <button class="cmd" id="headerFooterBtn" data-doc title="${t("tip.headerFooter")}"><i data-lucide="heading"></i><span>${t("lbl.headerFooter")}</span></button>
        </div>
        <div class="groupLabel">${t("grp.pageDesign")}</div>
      </div>

      <!-- ===== Ausfüllen & Signieren ===== -->
      <div data-ribbon="sign" class="ribbonGroup">
        <div class="groupBody">
          <button class="cmd" id="formPrevBtn" data-doc title="${t("tip.formPrev")}"><i data-lucide="chevron-left"></i><span>${t("lbl.prevField")}</span></button>
          <button class="cmd" id="formNextBtn" data-doc title="${t("tip.formNext")}"><i data-lucide="chevron-right"></i><span>${t("lbl.nextField")}</span></button>
          <button class="cmd" id="formValidateBtn" data-doc title="${t("tip.formValidate")}"><i data-lucide="clipboard-check"></i><span>${t("lbl.validate")}</span></button>
          <button class="cmd" id="formResetBtn" data-doc title="${t("tip.formReset")}"><i data-lucide="eraser"></i><span>${t("lbl.reset")}</span></button>
          <button class="cmd active" id="formHighlightBtn" title="${t("tip.formHighlight")}"><i data-lucide="eye"></i><span>${t("lbl.fieldHl")}</span></button>
        </div>
        <div class="groupLabel">${t("grp.form")}</div>
      </div>
      <div data-ribbon="sign" class="ribbonGroup">
        <div class="groupBody">
          <button class="cmd" id="signSignatureBtn" data-doc title="${t("tip.placeSignature")}"><i data-lucide="signature"></i><span>${t("btn.signature")}</span></button>
          <button class="cmd" id="signInitialsBtn" data-doc title="${t("tip.placeInitials")}"><i data-lucide="pen-tool"></i><span>${t("btn.initials")}</span></button>
          <div class="menuHost">
            <button class="cmd menuTrigger" id="signMarksBtn" data-doc aria-haspopup="true" aria-expanded="false" title="${t("grp.fill")}"><i data-lucide="check"></i><span>${t("lbl.marks")}</span></button>
            <div class="popover hidden" role="menu">
              <button class="mi signMark" data-sign="check" role="menuitem"><i data-lucide="check"></i><span>${t("tip.markCheck")}</span></button>
              <button class="mi signMark" data-sign="cross" role="menuitem"><i data-lucide="x"></i><span>${t("tip.markCross")}</span></button>
              <button class="mi signMark" data-sign="dot" role="menuitem"><i data-lucide="dot"></i><span>${t("tip.markDot")}</span></button>
              <button class="mi signMark" data-sign="date" role="menuitem"><i data-lucide="calendar"></i><span>${t("tip.markDate")}</span></button>
              <button class="mi signMark" data-sign="text" role="menuitem"><i data-lucide="type"></i><span>${t("tip.signText")}</span></button>
            </div>
          </div>
          <label class="fieldCol" title="${t("tip.markColor")}">
            <input type="color" id="signColor" value="#1a3a8f" />
            <span class="fieldCaption"><i data-lucide="palette"></i>${t("lbl.color")}</span>
          </label>
          <button class="cmd" id="signManageBtn" title="${t("tip.manageSig")}"><i data-lucide="pen-line"></i><span>${t("lbl.manageSig")}</span></button>
        </div>
        <div class="groupLabel">${t("grp.signHand")}</div>
      </div>
      <div data-ribbon="sign" class="ribbonGroup">
        <div class="groupBody">
          <button class="cmd" id="dsignBtn" data-doc title="${t("tip.digitalSign")}"><i data-lucide="file-key-2"></i><span>${t("btn.digitalSign")}</span></button>
          <button class="cmd" id="dverifyBtn" data-doc title="${t("tip.verify")}"><i data-lucide="shield-check"></i><span>${t("btn.verify")}</span></button>
        </div>
        <div class="groupLabel">${t("grp.digitalId")}</div>
      </div>
      <div data-ribbon="sign" class="hintCol">
        <span id="formHint" class="modeHint">${t("hint.noDoc")}</span>
        <span id="signHint" class="modeHint">${t("hint.signPick")}</span>
      </div>

      <!-- ===== Konvertieren ===== -->
      <div data-ribbon="convert" class="ribbonGroup">
        <div class="groupBody">
          <button class="cmd" id="exportDocxBtn" data-doc title="${t("tip.expDocx")}"><i data-lucide="book-text"></i><span>Word</span></button>
          <button class="cmd" id="exportXlsxBtn" data-doc title="${t("tip.expXlsx")}"><i data-lucide="table"></i><span>Excel</span></button>
          <button class="cmd" id="exportImagesBtn" data-doc title="${t("tip.expImages")}"><i data-lucide="image"></i><span>${t("lbl.images")}</span></button>
          <button class="cmd" id="exportTextBtn" data-doc title="${t("tip.expText")}"><i data-lucide="file-text"></i><span>${t("btn.expText")}</span></button>
          <button class="cmd" id="exportHtmlBtn" data-doc title="${t("tip.expHtml")}"><i data-lucide="file-code-2"></i><span>HTML</span></button>
        </div>
        <div class="groupLabel">${t("grp.exportTo")}</div>
      </div>
      <div data-ribbon="convert" class="ribbonGroup">
        <div class="groupBody">
          <button class="cmd" id="imagesToPdfBtn" title="${t("tip.imagesToPdf")}"><i data-lucide="images"></i><span>${t("lbl.fromImages")}</span></button>
          <button class="cmd" id="officeToPdfBtn" title="${t("tip.officeToPdf")}"><i data-lucide="briefcase"></i><span>${t("lbl.fromOffice")}</span></button>
        </div>
        <div class="groupLabel">${t("grp.createPdf")}</div>
      </div>
      <div data-ribbon="convert" class="ribbonGroup">
        <div class="groupBody">
          <button class="cmd" id="pdfaSaveTabBtn" data-doc title="${t("tip.saveAsPdfa")}"><i data-lucide="file-check-2"></i><span>${t("lbl.saveAsPdfa")}</span></button>
          <button class="cmd" id="pdfaBatchBtn" title="${t("tip.convPdfa")}"><i data-lucide="files"></i><span>${t("lbl.pdfaBatch")}</span></button>
        </div>
        <div class="groupLabel">${t("grp.archive")}</div>
      </div>
      <div data-ribbon="convert" class="ribbonGroup">
        <div class="groupBody">
          <button class="cmd" id="ocrBtn" data-doc title="${t("tip.ocr")}"><i data-lucide="scan-text"></i><span>${t("btn.ocr")}</span></button>
          <button class="cmd" id="ocrBatchBtn" title="${t("tip.ocrBatch")}"><i data-lucide="layers"></i><span>${t("btn.ocrBatch")}</span></button>
        </div>
        <div class="groupLabel">${t("grp.ocr")}</div>
      </div>

      <!-- ===== Schützen ===== -->
      <div data-ribbon="protect" class="ribbonGroup">
        <div class="groupBody">
          <button class="cmd" id="protectBtn" data-doc title="${t("tip.protect")}"><i data-lucide="lock"></i><span>${t("btn.protect")}</span></button>
          <button class="cmd" id="unprotectBtn" title="${t("tip.unprotect")}" disabled><i data-lucide="lock-open"></i><span>${t("btn.unprotect")}</span></button>
        </div>
        <div class="groupLabel">${t("grp.password")}</div>
      </div>
      <div data-ribbon="protect" class="ribbonGroup">
        <div class="groupBody">
          <button class="cmd shapeTool" data-shape="redact" data-doc title="${t("tip.redactTool")}"><i data-lucide="box-select"></i><span>${t("lbl.redactArea")}</span></button>
          <button class="cmd" id="redactPatternBtn" data-doc title="${t("tip.redactPattern")}"><i data-lucide="regex"></i><span>${t("lbl.pattern")}</span></button>
          <button class="cmd" id="redactApplyBtn" disabled title="${t("tip.redactApply")}"><i data-lucide="check"></i><span>${t("lbl.applyRedact")}</span></button>
          <button class="cmd" id="redactDiscardBtn" disabled title="${t("tip.redactDiscard")}"><i data-lucide="x"></i><span>${t("lbl.discard")}</span></button>
        </div>
        <div class="groupLabel">${t("grp.redact")}</div>
      </div>
      <div data-ribbon="protect" class="ribbonGroup">
        <div class="groupBody">
          <button class="cmd" id="sanitizeBtn" data-doc title="${t("tip.sanitize")}"><i data-lucide="eraser"></i><span>${t("lbl.sanitize")}</span></button>
          <button class="cmd" id="findHiddenBtn" data-doc title="${t("tip.findHidden")}"><i data-lucide="eye-off"></i><span>${t("lbl.findHidden")}</span></button>
        </div>
        <div class="groupLabel">${t("grp.sanitize")}</div>
      </div>
      <span data-ribbon="protect" id="protectHint" class="modeHint"></span>
    </div>
  </header>
  <div class="body">
    <aside id="sidebar" class="sidebar hidden">
      <div class="tabs">
        <button class="tab active" data-tab="thumbs" title="${t("tab.thumbs")}"><i data-lucide="layout-grid"></i></button>
        <button class="tab" data-tab="outline" title="${t("tab.outline")}"><i data-lucide="list-tree"></i></button>
        <button class="tab" data-tab="attachments" title="${t("tab.files")}"><i data-lucide="paperclip"></i></button>
      </div>
      <div id="thumbsPanel" class="panel"></div>
      <div id="outlinePanel" class="panel hidden"></div>
      <div id="attachmentsPanel" class="panel hidden"></div>
    </aside>
    <div id="viewer" class="viewer">
      <div id="ocrBanner" class="ocrBanner hidden">
        <i data-lucide="scan-text"></i>
        <span>${t("ocr.banner")}</span>
        <button id="ocrBannerStart">${t("ocr.bannerStart")}</button>
        <button id="ocrBannerClose" class="propsClose" title="${t("tip.close")}">✕</button>
      </div>
      <div id="pageList" class="pageList"></div>
    </div>
    <div id="emptyState" class="welcome">
      <div class="welcomeCard">
        <img class="welcomeLogo" src="${logoText}" alt="Docura PDF" draggable="false" />
        <button id="welcomeOpenBtn" class="primaryBtn"><i data-lucide="folder-open"></i>${t("welcome.open")}</button>
        <div class="welcomeHint">${t("welcome.drop")}</div>
        <div class="welcomeActions">
          <button id="welcomeMergeBtn"><i data-lucide="merge"></i><span>${t("welcome.merge")}</span></button>
          <button id="welcomeImagesBtn"><i data-lucide="images"></i><span>${t("welcome.images")}</span></button>
          <button id="welcomeOfficeBtn"><i data-lucide="briefcase"></i><span>${t("welcome.office")}</span></button>
        </div>
        <div class="welcomeRecentTitle">${t("welcome.recent")}</div>
        <div id="welcomeRecent" class="welcomeRecent"></div>
      </div>
    </div>
    <aside id="commentPanel" class="commentPanel hidden">
      <div class="commentHeader">
        <strong>${t("comments.title")}</strong>
        <button id="commentReloadBtn" title="${t("tip.refresh")}"><i data-lucide="refresh-cw"></i></button>
        <button id="commentExportBtn" title="${t("tip.exportXfdf")}"><i data-lucide="file-down"></i></button>
        <button id="commentImportBtn" title="${t("tip.importXfdf")}"><i data-lucide="file-up"></i></button>
        <button id="commentCloseBtn" class="propsClose" title="${t("tip.close")}">✕</button>
      </div>
      <div class="commentFilters">
        <select id="commentKindFilter">
          <option value="">${t("filter.allKinds")}</option>
          <option value="Highlight">${t("kind.Highlight")}</option>
          <option value="Underline">${t("kind.Underline")}</option>
          <option value="StrikeOut">${t("kind.StrikeOut")}</option>
          <option value="Squiggly">${t("kind.Squiggly")}</option>
          <option value="Square">${t("kind.Square")}</option>
          <option value="Circle">${t("kind.Circle")}</option>
          <option value="Line">${t("kind.Line")}</option>
          <option value="Polygon">${t("kind.Polygon")}</option>
          <option value="PolyLine">${t("kind.PolyLine")}</option>
          <option value="Ink">${t("kind.Ink")}</option>
          <option value="FreeText">${t("kind.FreeText")}</option>
          <option value="Stamp">${t("kind.Stamp")}</option>
          <option value="Text">${t("kind.Text")}</option>
        </select>
        <input id="commentAuthorFilter" type="text" placeholder="${t("ph.author")}" />
        <label class="commentChk"><input type="checkbox" id="commentHasContent" /> ${t("lbl.withComment")}</label>
      </div>
      <div id="commentList" class="commentList"></div>
    </aside>
    <div id="mergePane" class="mergePane hidden">
      <div class="mergeHeader">
        <strong>${t("merge.title")}</strong>
        <button id="mergeAddBtn" title="${t("tip.pickPdfs")}"><i data-lucide="plus"></i><span>${t("btn.addFiles")}</span></button>
        <span class="mergeHint">${t("hint.merge")}</span>
        <button id="mergeCancelBtn" title="${t("tip.cancel")}">${t("tip.cancel")}</button>
        <button id="mergeCommitBtn" class="primaryBtn" disabled title="${t("tip.buildMerged")}"><i data-lucide="merge"></i><span>${t("btn.merge")}</span></button>
      </div>
      <div id="mergeList" class="mergeList"></div>
    </div>
    <div id="convertPane" class="mergePane convertPane hidden">
      <div class="mergeHeader">
        <strong>${t("convert.title")}</strong>
        <button id="convertAddBtn" title="${t("tip.pickPdfs")}"><i data-lucide="plus"></i><span>${t("btn.addFiles")}</span></button>
        <select id="convertLevel" title="${t("tip.pdfaLevel")}">
          <option value="1b">PDF/A-1b</option>
          <option value="2b" selected>PDF/A-2b</option>
          <option value="3b">PDF/A-3b</option>
          <option value="4">PDF/A-4</option>
        </select>
        <button id="convertOutBtn" title="${t("tip.pickOutFolder")}"><i data-lucide="folder-open"></i><span>${t("btn.outFolder")}</span></button>
        <span id="convertOutLabel" class="mergeHint">${t("convert.output")}</span>
        <button id="convertCancelBtn" title="${t("tip.cancel")}">${t("tip.cancel")}</button>
        <button id="convertCommitBtn" class="primaryBtn" disabled title="${t("tip.convertAll")}"><i data-lucide="check"></i><span>${t("btn.convert")}</span></button>
      </div>
      <div id="convertList" class="mergeList"></div>
    </div>
    <div id="searchPanel" class="searchPanel hidden">
      <input id="searchInput" type="text" placeholder="${t("ph.find")}" />
      <label><input id="searchCase" type="checkbox" /> ${t("lbl.case")}</label>
      <button id="searchPrev" title="${t("tip.prevMatch")}"><i data-lucide="chevron-left"></i></button>
      <button id="searchNext" title="${t("tip.nextMatch")}"><i data-lucide="chevron-right"></i></button>
      <span id="searchCount"></span>
      <button id="searchClose" title="${t("tip.closeEsc")}"><i data-lucide="x"></i></button>
    </div>
  </div>
  <footer class="statusBar">
    <button id="sidebarBtn" class="sbBtn" title="${t("tip.sidebar")}"><i data-lucide="panel-left"></i></button>
    <span id="status" class="status" role="status" aria-live="polite"></span>
    <div class="sbGroup" data-doc>
      <button id="prevPage" class="sbBtn" title="${t("tip.prevPage")}"><i data-lucide="chevron-left"></i></button>
      <input id="pageInput" type="number" min="1" value="1" title="${t("tip.goto")}" />
      <span id="pageTotal">/ 0</span>
      <button id="nextPage" class="sbBtn" title="${t("tip.nextPage")}"><i data-lucide="chevron-right"></i></button>
    </div>
    <span class="sbSep"></span>
    <div class="sbGroup" data-doc>
      <button id="zoomOut" class="sbBtn" title="${t("tip.zoomOut")}"><i data-lucide="zoom-out"></i></button>
      <select id="zoomSelect" title="${t("tip.zoom")}">
        <option value="custom" id="zoomCustomOpt">100%</option>
        <option value="width">${t("fit.width")}</option>
        <option value="page">${t("fit.page")}</option>
        <option value="0.5">50%</option>
        <option value="0.75">75%</option>
        <option value="1">100%</option>
        <option value="1.25">125%</option>
        <option value="1.5">150%</option>
        <option value="2">200%</option>
        <option value="4">400%</option>
      </select>
      <button id="zoomIn" class="sbBtn" title="${t("tip.zoomIn")}"><i data-lucide="zoom-in"></i></button>
      <button id="fitWidthBtn" class="sbBtn" title="${t("tip.fitWidth")}"><i data-lucide="move-horizontal"></i></button>
      <button id="fitPageBtn" class="sbBtn" title="${t("tip.fitPage")}"><i data-lucide="rectangle-vertical"></i></button>
    </div>
  </footer>
  <div id="updateHint" class="updateHint hidden" role="status" aria-live="polite">
    <div class="updateHintHead">
      <i data-lucide="arrow-down-to-line"></i>
      <strong id="updateHintTitle"></strong>
      <button id="updateHintClose" class="propsClose" title="${t("tip.close")}">✕</button>
    </div>
    <div id="updateHintInstalled" class="updateHintSub"></div>
    <div id="updateHintNotes" class="updateHintNotes"></div>
    <div class="updateHintActions">
      <button id="updateHintRelease">${t("upd.releaseNotes")}</button>
      <button id="updateHintDownload" class="primaryBtn">${t("upd.download")}</button>
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
    <div class="propsModal settingsModal">
      <div class="propsHeader"><strong>${t("set.title")}</strong><button id="settingsClose" class="propsClose">✕</button></div>
      <div class="settingsLayout">
        <nav class="settingsNav" id="settingsNav">
          <button class="settingsNavItem active" data-sec="secAppearance">${t("set.appearance")}</button>
          <button class="settingsNavItem" data-sec="secPerformance">${t("set.performance")}</button>
          <button class="settingsNavItem" data-sec="secPdfa">${t("set.pdfaExport")}</button>
          <button class="settingsNavItem" data-sec="secVprinter">${t("set.vprinter")}</button>
          <button class="settingsNavItem" data-sec="secSystem">${t("set.system")}</button>
          <button class="settingsNavItem" data-sec="secUpdates">${t("set.updates")}</button>
        </nav>
        <div class="settingsBody" id="settingsBody">
          <section id="secAppearance" class="wmPanel settingsSection">
            <h3 style="margin:0 0 0.3rem 0;font-size:0.95rem;">${t("set.appearance")}</h3>
            <label>${t("set.theme")}
              <select id="themeSelect">
                <option value="light">${t("theme.light")}</option>
                <option value="dark">${t("theme.dark")}</option>
                <option value="docura">${t("theme.docura")}</option>
                <option value="contrast">${t("theme.contrast")}</option>
              </select>
            </label>
            <label>${t("set.language")}
              <select id="langSelect">
                <option value="de">Deutsch</option>
                <option value="en">English</option>
              </select>
            </label>
            <label>${t("set.defaultZoom")}
              <select id="defaultZoomSelect">
                <option value="page">${t("fit.page")}</option>
                <option value="width">${t("fit.width")}</option>
                <option value="actual">${t("fit.actual")}</option>
              </select>
            </label>
            <div style="font-size:0.78rem;color:var(--text-3);">${t("set.themeHint")}</div>
          </section>
          <section id="secPerformance" class="wmPanel settingsSection">
            <h3 style="margin:0 0 0.3rem 0;font-size:0.95rem;">${t("set.performance")}</h3>
            <label>${t("set.cacheSize")}
              <input id="cacheMbInput" type="number" min="16" max="4096" step="16" value="256" style="width:6rem;" />
            </label>
            <div style="font-size:0.78rem;color:var(--text-3);">${t("set.cacheHint")}</div>
          </section>
          <section id="secPdfa" class="wmPanel settingsSection">
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
            <div class="wmActions"><button id="pdfaSaveBtn" class="primaryBtn">${t("btn.savePdfaNow")}</button><button id="gsRecheckBtn">${t("btn.recheckGs")}</button></div>
          </section>
          <section id="secVprinter" class="wmPanel settingsSection">
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
          </section>
          <section id="secSystem" class="wmPanel settingsSection">
            <h3 style="margin:0 0 0.3rem 0;font-size:0.95rem;">${t("set.system")}</h3>
            <label class="settingsCheck"><input id="runBackgroundCheck" type="checkbox" /> ${t("set.runInBackground")}</label>
            <div style="font-size:0.78rem;color:var(--text-3);">${t("set.runInBackgroundHint")}</div>
            <label class="settingsCheck" style="margin-top:0.6rem;"><input id="autostartCheck" type="checkbox" /> ${t("set.autostart")}</label>
            <div style="font-size:0.78rem;color:var(--text-3);">${t("set.autostartHint")}</div>
          </section>
          <section id="secUpdates" class="wmPanel settingsSection">
            <h3 style="margin:0 0 0.3rem 0;font-size:0.95rem;">${t("set.updates")}</h3>
            <div id="updateStatus" style="font-size:0.85rem;color:var(--text-2);"></div>
            <label class="settingsCheck"><input id="updateAutoCheck" type="checkbox" /> ${t("set.updateAuto")}</label>
            <div style="font-size:0.78rem;color:var(--text-3);">${t("set.updateHint")}</div>
            <div class="wmActions"><button id="updateCheckBtn">${t("btn.checkUpdates")}</button><button id="updateDownloadBtn" class="primaryBtn hidden">${t("upd.download")}</button></div>
          </section>
        </div>
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
      <div class="wmActions" style="padding: 0.5rem 0.8rem;"><button id="wmCommit" class="primaryBtn">${t("btn.apply")}</button><button id="wmCancel">${t("tip.cancel")}</button></div>
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
        <div class="wmActions"><button id="hfCommit" class="primaryBtn">${t("btn.apply")}</button><button id="hfCancel">${t("tip.cancel")}</button></div>
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
        <div class="wmActions"><button id="sigClear">${t("btn.clear")}</button><button id="sigSave" class="primaryBtn">${t("btn.save")}</button><button id="sigCancel">${t("tip.cancel")}</button></div>
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
        <div class="wmActions"><button id="dsignCommit" class="primaryBtn">${t("btn.sign")}</button><button id="dsignCancel">${t("tip.cancel")}</button></div>
      </div>
    </div>
  </div>
  <div id="dverifyOverlay" class="propsOverlay hidden">
    <div class="propsModal">
      <div class="propsHeader"><strong>${t("dverify.title")}</strong><button id="dverifyClose" class="propsClose">✕</button></div>
      <div id="dverifyBody" class="propsPanel"></div>
    </div>
  </div>
  <div id="pwOverlay" class="propsOverlay hidden">
    <div class="propsModal" style="width:24rem;">
      <div class="propsHeader"><strong>${t("pw.title")}</strong><button id="pwClose" class="propsClose">✕</button></div>
      <div class="propsPanel wmPanel">
        <span id="pwMessage" class="sigHint"></span>
        <label>${t("lbl.password")}<input id="pwInput" type="password" autocomplete="off" /></label>
        <div class="wmActions"><button id="pwOk" class="primaryBtn">${t("btn.ok")}</button><button id="pwCancel">${t("tip.cancel")}</button></div>
      </div>
    </div>
  </div>
  <div id="protectOverlay" class="propsOverlay hidden">
    <div class="propsModal" style="width:26rem;">
      <div class="propsHeader"><strong>${t("protect.title")}</strong><button id="protectClose" class="propsClose">✕</button></div>
      <div class="propsPanel wmPanel">
        <label>${t("protect.userPw")}<input id="protectUserPw" type="password" autocomplete="off" placeholder="${t("ph.openPw")}" /></label>
        <label>${t("protect.ownerPw")}<input id="protectOwnerPw" type="password" autocomplete="off" placeholder="${t("ph.ownerPw")}" /></label>
        <span class="sigHint">${t("protect.hint")}</span>
        <strong style="font-size:0.85rem;">${t("protect.permsTitle")}</strong>
        <label class="commentChk"><input type="checkbox" id="permPrint" checked /> ${t("perm.print")}</label>
        <label class="commentChk"><input type="checkbox" id="permModify" checked /> ${t("perm.modify")}</label>
        <label class="commentChk"><input type="checkbox" id="permCopy" checked /> ${t("perm.copy")}</label>
        <label class="commentChk"><input type="checkbox" id="permAnnotate" checked /> ${t("perm.annotate")}</label>
        <div class="wmActions">
          <button id="protectCommit" class="primaryBtn">${t("btn.protectSave")}</button>
          <button id="protectCancel">${t("tip.cancel")}</button>
        </div>
      </div>
    </div>
  </div>
  <div id="patternOverlay" class="propsOverlay hidden">
    <div class="propsModal" style="width:26rem;">
      <div class="propsHeader"><strong>${t("pattern.title")}</strong><button id="patternClose" class="propsClose">✕</button></div>
      <div class="propsPanel wmPanel">
        <label class="commentChk"><input type="checkbox" id="patternEmail" checked /> ${t("pattern.email")}</label>
        <label class="commentChk"><input type="checkbox" id="patternPhone" /> ${t("pattern.phone")}</label>
        <label>${t("pattern.custom")}<input id="patternCustom" type="text" autocomplete="off" spellcheck="false" placeholder="${t("ph.pattern")}" /></label>
        <span class="sigHint">${t("pattern.hint")}</span>
        <div class="wmActions">
          <button id="patternCommit" class="primaryBtn">${t("pattern.search")}</button>
          <button id="patternCancel">${t("tip.cancel")}</button>
        </div>
      </div>
    </div>
  </div>
  <div id="exportImgOverlay" class="propsOverlay hidden">
    <div class="propsModal" style="width:24rem;">
      <div class="propsHeader"><strong>${t("export.title")}</strong><button id="exportImgClose" class="propsClose">✕</button></div>
      <div class="propsPanel wmPanel">
        <label>${t("export.format")}<select id="expImgFormat"><option value="png">PNG</option><option value="jpg">JPG</option></select></label>
        <label>${t("export.dpi")}<select id="expImgDpi"><option>96</option><option selected>150</option><option>300</option><option>600</option></select></label>
        <label>${t("export.scope")}<select id="expImgScope">
          <option value="all">${t("export.scopeAll")}</option>
          <option value="current">${t("export.scopeCurrent")}</option>
          <option value="range">${t("export.scopeRange")}</option>
        </select></label>
        <div id="expImgRange" style="display:none;gap:0.5rem;">
          <label style="flex:1">${t("export.from")}<input id="expImgFrom" type="number" min="1" /></label>
          <label style="flex:1">${t("export.to")}<input id="expImgTo" type="number" min="1" /></label>
        </div>
        <div class="wmActions">
          <button id="exportImgCommit" class="primaryBtn">${t("btn.export")}</button>
          <button id="exportImgCancel">${t("tip.cancel")}</button>
        </div>
      </div>
    </div>
  </div>
  <div id="ocrOverlay" class="propsOverlay hidden">
    <div class="propsModal" style="width:26rem;">
      <div class="propsHeader"><strong>${t("ocr.title")}</strong><button id="ocrDlgClose" class="propsClose">✕</button></div>
      <div class="propsPanel wmPanel">
        <label>${t("ocr.language")}<select id="ocrLang"></select></label>
        <label>${t("export.scope")}<select id="ocrScope">
          <option value="all">${t("export.scopeAll")}</option>
          <option value="current">${t("export.scopeCurrent")}</option>
          <option value="range">${t("export.scopeRange")}</option>
        </select></label>
        <div id="ocrRange" style="display:none;gap:0.5rem;">
          <label style="flex:1">${t("export.from")}<input id="ocrFrom" type="number" min="1" /></label>
          <label style="flex:1">${t("export.to")}<input id="ocrTo" type="number" min="1" /></label>
        </div>
        <label class="commentChk"><input type="checkbox" id="ocrPreprocess" checked /> ${t("ocr.preprocess")}</label>
        <label class="commentChk"><input type="checkbox" id="ocrSkipText" checked /> ${t("ocr.skipText")}</label>
        <div class="wmActions">
          <button id="ocrCommit" class="primaryBtn">${t("ocr.start")}</button>
          <button id="ocrCancel">${t("tip.cancel")}</button>
        </div>
      </div>
    </div>
  </div>
  <div id="hiddenOverlay" class="propsOverlay hidden">
    <div class="propsModal" style="width:30rem;">
      <div class="propsHeader"><strong>${t("hidden.title")}</strong><button id="hiddenClose" class="propsClose">✕</button></div>
      <div class="propsPanel wmPanel" id="hiddenBody" style="max-height:60vh;overflow:auto;"></div>
    </div>
  </div>
  <div id="splitOverlay" class="propsOverlay hidden">
    <div class="propsModal" style="width:26rem;">
      <div class="propsHeader"><strong>${t("split.title")}</strong><button id="splitClose" class="propsClose" title="${t("tip.close")}">✕</button></div>
      <div class="propsPanel wmPanel">
        <label class="commentChk"><input type="radio" name="splitMode" id="splitModeEvery" value="every" checked /> ${t("split.every")}</label>
        <label class="splitIndent">${t("split.pagesPerFile")}<input id="splitEvery" type="number" min="1" step="1" value="1" /></label>
        <label class="commentChk"><input type="radio" name="splitMode" id="splitModeRanges" value="ranges" /> ${t("split.ranges")}</label>
        <label class="splitIndent">${t("split.rangesLabel")}<input id="splitRanges" type="text" autocomplete="off" spellcheck="false" /></label>
        <span id="splitPreview" class="sigHint"></span>
        <div class="wmActions">
          <button id="splitCommit" class="primaryBtn">${t("split.commit")}</button>
          <button id="splitCancel">${t("tip.cancel")}</button>
        </div>
      </div>
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
  /* Brand theme in the logo's violet palette. */
  body.theme-docura {
    --accent:#7c3aed; --accent-strong:#6d28d9; --accent-tint:#ede9fe;
    --bg:#ffffff; --bg-2:#f5f3ff; --bg-3:#ece6fc; --hover:#f1ecfd;
    --border:#e4dcf8; --border-2:#d2c6f0;
    --text:#241e3a; --text-2:#5b5374; --text-3:#8d86a3;
    --canvas:#3f3955; --danger:#d23b3b;
  }
  /* High-contrast theme (accessibility): pure black/white with a bright accent
     and solid borders. */
  body.theme-contrast {
    --accent:#ffd400; --accent-strong:#fff34d; --accent-tint:#3a3300;
    --bg:#000000; --bg-2:#000000; --bg-3:#0d0d0d; --hover:#252100;
    --border:#ffffff; --border-2:#ffffff;
    --text:#ffffff; --text-2:#ffffff; --text-3:#e6e6e6;
    --canvas:#000000; --danger:#ff5a5a;
  }
  body.theme-contrast button, body.theme-contrast select, body.theme-contrast input,
  body.theme-contrast textarea { border:1px solid #fff !important; }
  body.theme-contrast .pageSlot { box-shadow:0 0 0 2px #fff; }
  /* Keyboard focus must be clearly visible everywhere. */
  :focus-visible { outline:2px solid var(--accent); outline-offset:2px; }
  body.theme-contrast :focus-visible { outline:3px solid #ffd400; outline-offset:2px; }
  /* Dark-theme tweaks for the few light-pastel accents. */
  body.theme-dark .deleteAction:not(:disabled) { background:#3a2a2a !important; color:#e09f9f !important; border-color:#5a3a3a !important; }
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
  /* The tab row doubles as the window title bar (decorations are off):
     tabs left, drag area with the document name in the middle, window
     controls right. data-tauri-drag-region handles move + double-click
     maximize; spans inside get pointer-events:none so hits land on the
     element carrying the attribute. */
  .ribbonTabs { display:flex; gap:0.1rem; background:var(--bg-2);
                border-bottom:1px solid var(--border); padding: 0 0 0 0.2rem; align-items:stretch;
                min-height:34px; user-select:none; -webkit-user-select:none; }
  /* Two-tier chrome: row 1 = title bar (app icon + quick-access toolbar +
     title + window controls); row 2 = tab strip; row 3 = ribbon body. */
  .titleBar { display:flex; gap:0.1rem; background:var(--bg-2); align-items:center;
              padding: 0 0 0 0.2rem; min-height:34px; user-select:none; -webkit-user-select:none; }
  .tabStrip { display:flex; gap:0.1rem; align-items:stretch; background:var(--bg-2);
              border-bottom:1px solid var(--border); padding: 0 0.3rem; }
  .appGlyph { display:inline-flex; align-items:center; padding:0 0.45rem 0 0.3rem; }
  .appGlyph img { width:16px; height:16px; pointer-events:none; }
  .titleDrag { flex:1; min-width:0; display:flex; align-items:center; justify-content:center; }
  .titleDrag span { font-size:0.78rem; color:var(--text-3); white-space:nowrap; overflow:hidden;
                    text-overflow:ellipsis; pointer-events:none; padding:0 0.6rem; }
  .winControls { display:flex; align-items:stretch; }
  .winControls button { width:44px; border:none; background:transparent; color:var(--text-2);
                        padding:0; display:inline-flex; align-items:center; justify-content:center;
                        border-radius:0; cursor:default; font:inherit; }
  .winControls button:hover { background:var(--bg-3); color:var(--text); }
  .winControls #winClose:hover { background:#e81123; color:#fff; }
  .winControls svg.lucide { width:15px; height:15px; stroke-width:1.6; }
  .winBtn { width:38px; align-self:stretch; border:none; background:transparent; color:var(--text-2);
            padding:0; display:inline-flex; align-items:center; justify-content:center;
            border-radius:0; cursor:pointer; font:inherit; }
  .winBtn:hover { background:var(--bg-3); color:var(--text); }
  .winBtn svg.lucide { width:16px; height:16px; }
  /* Foxit-style flat text tabs: no box fill, active marked by an accent
     underline + accent text. */
  .ribbonTab { background:transparent; color:var(--text-2); border:none; padding:0.4rem 0.95rem;
               cursor:pointer; font:inherit; font-size:0.85rem; border-radius:0;
               border-bottom:2px solid transparent; }
  .ribbonTab:hover { background:transparent; color:var(--text); }
  .ribbonTab.active { background:transparent; color:var(--accent); font-weight:600;
                      border-bottom:2px solid var(--accent); }
  /* Ribbon display toggle at the right end of the tab strip: chevron down =
     ribbon always shown, chevron up = auto-hide (body shows on tab hover). */
  .ribbonPin { margin-left:auto; align-self:center; width:28px; height:24px; padding:0; border:none;
               border-radius:4px; background:transparent; color:var(--text-3); cursor:pointer;
               display:inline-flex; align-items:center; justify-content:center; }
  .ribbonPin:hover { background:var(--bg-3); color:var(--text); }
  .ribbonPin svg.lucide { width:15px; height:15px; }
  /* Auto-hide: the body leaves the layout and slides in as an overlay, so
     the viewer keeps its size (no re-render) while the ribbon peeks. */
  .ribbon.autoHide { z-index:39; }
  .ribbon.autoHide .ribbonBody { position:absolute; top:100%; left:0; right:0;
      border-bottom:1px solid var(--border); box-shadow:0 8px 18px rgba(20,30,50,0.16);
      visibility:hidden; opacity:0; transform:translateY(-6px); pointer-events:none;
      transition: opacity .12s ease, transform .12s ease, visibility 0s linear .12s; }
  .ribbon.autoHide.peek .ribbonBody { visibility:visible; opacity:1; transform:none; pointer-events:auto;
      transition: opacity .12s ease, transform .12s ease; }
  @media (prefers-reduced-motion: reduce) {
    .ribbon.autoHide .ribbonBody, .ribbon.autoHide.peek .ribbonBody { transition:none; transform:none; }
  }
  /* Quick-access toolbar in the title bar: compact icon-only cluster
     (open/save/print/undo/redo/page-nav). */
  .qat { display:inline-flex; align-items:center; gap:0.1rem; padding:0 0.35rem;
         margin-left:0.2rem; border-left:1px solid var(--border); }
  .qat button { background:transparent; border:1px solid transparent; border-radius:4px;
                color:var(--text-2); cursor:pointer; font:inherit; padding:0;
                width:2rem; height:2rem; box-sizing:border-box;
                display:inline-flex; align-items:center; justify-content:center; }
  .qat button:hover:not(:disabled) { background:var(--bg-3); color:var(--text); }
  .qat button:disabled { opacity:0.4; cursor:default; }
  .qat svg.lucide { width:16px; height:16px; }
  .qat input { background:var(--bg); color:var(--text); border:1px solid var(--border-2);
               border-radius:4px; padding:0.15rem 0.3rem; font:inherit; text-align:center; }
  .qat #pageTotal { font-size:0.8rem; color:var(--text-2); padding:0 0.1rem; }
  /* Dropdown caret. Compact triggers (QAT/recent) get it inline after the icon;
     labeled .cmd triggers get it after the caption. */
  button.hasCaret::after, .menuTrigger:not(.cmd)::after { content:"\\25be"; font-size:0.7em;
                                                          margin-left:0.15rem; opacity:0.7; }
  .cmd.menuTrigger > span::after { content:" \\25be"; opacity:0.55; font-size:0.9em; }
  /* Foxit-style command button: icon stacked over a caption. Uniform box so the
     whole ribbon reads as an even grid (same width/height, centered content). */
  button.cmd { flex-direction:column; justify-content:center; gap:0.2rem;
               width:auto; min-width:3.9rem; max-width:8.5rem; height:3.5rem; padding:0.2rem 0.45rem; line-height:1.15;
               border-radius:5px; box-sizing:border-box; }
  button.cmd svg.lucide { width:18px; height:18px; }
  /* One-line captions: wide enough for the German labels, ellipsis as a
     last resort instead of clipped second lines. */
  button.cmd > span { font-size:0.68rem; color:var(--text-2); text-align:center; max-width:100%;
                      white-space:nowrap; overflow:hidden; text-overflow:ellipsis; display:block; }
  button.cmd:hover:not(:disabled) > span, button.cmd.active > span { color:inherit; }
  .ribbonBody { padding: 0.3rem 0.5rem 0.2rem; display:flex; gap: 0.2rem; align-items:stretch;
                flex-wrap: wrap; background:var(--bg); min-height:4.6rem; box-sizing:border-box; }
  .ribbonBody button, .ribbonBody select, .ribbonBody input {
    background:var(--bg); color:var(--text); border:1px solid var(--border-2); border-radius:4px;
    padding:0.25rem 0.5rem; font: inherit;
  }
  .ribbonBody > button, .ribbonBody .groupBody > button, .ribbonBody .menuHost > button.cmd {
    background:transparent; border:1px solid transparent;
  }
  .ribbonBody button:hover:not(:disabled) { background:var(--hover); border-color:var(--border-2); }
  .ribbonBody button:disabled { opacity:0.4; cursor:default; }
  .ribbonBody button.active { background:var(--accent-tint); color:var(--accent-strong);
                              border-color:#bcd4f6; }
  .ribbonBody input[type=color] { background:var(--bg) !important; border:1px solid var(--border-2) !important; }
  .ribbonBody input:focus, .ribbonBody select:focus { outline:2px solid var(--accent-tint); border-color:var(--accent); }
  /* Lucide SVG icons: stroke follows the text color of the host button. */
  svg.lucide { width:16px; height:16px; stroke-width:2; flex:none; pointer-events:none; vertical-align:-3px; }
  .ribbonBody button, .commentHeader button, .mergeHeader button,
  .ribbonTab.settingsTab {
    display:inline-flex; align-items:center; justify-content:center; gap:0.3rem;
  }
  .signMark svg.lucide { width:15px; height:15px; }
  .commentItem .kind svg.lucide { width:13px; height:13px; }
  body.wheelZoomMode .viewer { cursor: grab; }
  body.wheelZoomMode.panning .viewer { cursor: grabbing; }
  body.wheelZoomMode .selectLayer, body.wheelZoomMode .drawLayer { pointer-events: none; }
  body.wheelZoomMode .pageSlot, body.wheelZoomMode .pageSlot * { user-select: none; -webkit-user-select: none; }
  /* setRibbonTab() marks every ribbon child of an inactive tab with .offTab. */
  .ribbonBody > .offTab { display:none !important; }
  .ribbonLabel { color:var(--text-3); font-size:0.85rem; margin-left:0.2rem; }
  /* Group = a divider-separated cluster of command buttons (no caption row;
     each button carries its own Foxit-style caption). */
  .ribbonGroup { display:inline-flex; flex-direction:column; align-items:stretch; align-self:stretch;
                 padding: 0 0.4rem; border-right:1px solid var(--border); }
  .ribbonGroup .groupBody { display:flex; align-items:center; gap:0.05rem; flex:1; }
  .groupLabel { font-size:0.62rem; color:var(--text-3); text-align:center; padding-top:0.1rem;
                white-space:nowrap; letter-spacing:0.2px; }
  /* Contextual hints fill the space right of the groups and shrink first,
     so they never push the ribbon onto a second row. */
  .hintCol { display:flex; flex-direction:column; justify-content:center; gap:0.2rem;
             flex:1 1 0; min-width:0; overflow:hidden; padding-left:0.3rem; }
  .ribbonBody > .modeHint { flex:1 1 0; min-width:0; align-self:center; padding-left:0.3rem; }
  .ribbonBody .modeHint { display:-webkit-box; -webkit-line-clamp:3; -webkit-box-orient:vertical;
                          overflow:hidden; font-size:0.78rem; line-height:1.3; }
  .hintCol .modeHint { -webkit-line-clamp:2; }
  .fieldCol { display:inline-flex; flex-direction:column; align-items:center; justify-content:center;
              gap:0.15rem; height:3.5rem; padding:0 0.25rem; box-sizing:border-box; }
  .fieldCol select { font-size:0.78rem; padding:0.1rem 0.25rem; }
  .fieldCol input[type=color] { height:1.5rem; width:2rem; padding:0; cursor:pointer; }
  .fieldCaption { font-size:0.62rem; color:var(--text-2); display:inline-flex; align-items:center;
                  gap:0.2rem; }
  .fieldCaption svg.lucide { width:12px; height:12px; }
  .modeHint { color:var(--text-2); font-size:0.82rem; margin-left:0.3rem; }
  .menuHost { position:relative; display:inline-block; }
  .popover { position:absolute; top:calc(100% + 4px); left:0; background:var(--bg);
             border:1px solid var(--border-2); border-radius:6px; box-shadow:0 6px 20px rgba(20,30,50,0.18);
             padding:0.25rem; display:flex; flex-direction:column; min-width:120px; z-index:100; }
  .popover.hidden { display:none; }
  .popover button { background:transparent; color:var(--text); border:none; padding:0.4rem 0.6rem;
                    text-align:left; border-radius:4px; cursor:pointer; font-size:0.9rem; }
  .popover button:hover { background:var(--hover); }
  /* Popover menu items: icon + label in a row; reuse the active-tint for the
     currently-selected tool. */
  .popover .mi { display:flex; align-items:center; justify-content:flex-start; gap:0.5rem;
                 width:100%; white-space:nowrap; }
  .popover .mi svg.lucide { width:16px; height:16px; }
  .popover .mi > span { font-size:0.85rem; }
  /* Form-style popover (text options): label left, control right. */
  .popoverForm { min-width:210px; gap:0.15rem; }
  .popoverForm .popRow { display:flex; align-items:center; justify-content:space-between;
                         gap:0.6rem; padding:0.2rem 0.45rem; font-size:0.82rem; color:var(--text);
                         cursor:default; }
  .popoverForm .popRow input[type=color] { height:1.4rem; width:2rem; padding:0; }
  .menuTrigger[aria-expanded="true"] { background:var(--bg-3); }
  .cmd.menuTrigger[aria-expanded="true"] { background:var(--accent-tint); }
  .thumb.selected { outline:2px solid var(--accent); box-shadow: 0 0 0 3px rgba(43,108,223,0.3); }
  .deleteAction { background:#fbecec !important; color:#c0392b !important; border:1px solid #f0c4c4 !important; }
  .deleteAction:not(:disabled):hover { background:#c0392b !important; color:#fff !important; border-color:#c0392b !important; }
  .deleteAction:disabled { opacity:0.35; background:transparent !important; border-color:transparent !important; }
  .sep { display:inline-block; width:1px; height:20px; background:var(--border-2); margin: 0 0.2rem; }
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
  .tab { flex:1; padding:0.45rem; background:var(--bg-2); color:var(--text-2); border:none; cursor:pointer; border-bottom:2px solid transparent;
         display:inline-flex; align-items:center; justify-content:center; }
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
  /* "safe center": once a zoomed page is wider than the viewer it aligns to
     the start instead of overflowing to the left, where scrolling (and the
     hand tool) could never reach it. */
  .pageList { display:flex; flex-direction:column; align-items:safe center; gap: 1rem; box-sizing:border-box; }
  .pageList.double { flex-flow: row wrap; justify-content:safe center; align-content:flex-start; gap: 0.6rem 0.6rem; }
  .pageList.single > .pageSlot:not(.current) { display:none; }
  .pageSlot { position:relative; background:white; box-shadow: 0 2px 10px rgba(0,0,0,0.35); flex: 0 0 auto; }
  .pageSlot img.page { display:block; width:100%; height:100%; }
  /* Sharp render of the visible window at high zoom, over the upscaled page. */
  .pageSlot img.detail { position:absolute; display:block; pointer-events:none; }
  body.dark .pageSlot img.detail { filter: invert(0.92) hue-rotate(180deg); }
  /* Loading skeleton for never-rendered pages: a blank sheet with a soft
     shimmer sweep (replaces the old diagonal hatch). */
  .pageSlot .placeholder { position:absolute; inset:0; display:flex; align-items:center; justify-content:center;
                           color:#b8b8bd; font-size:0.8rem; background:#f4f4f5; overflow:hidden; }
  .pageSlot .placeholder::after { content:""; position:absolute; inset:0; pointer-events:none;
                           background: linear-gradient(100deg, transparent 35%, rgba(255,255,255,0.7) 50%, transparent 65%);
                           transform: translateX(-100%); animation: skeletonSweep 1.3s ease-in-out infinite; }
  @keyframes skeletonSweep { to { transform: translateX(100%); } }
  .theme-dark .pageSlot .placeholder { background:#2a2d31; color:#5a5e66; }
  .theme-dark .pageSlot .placeholder::after { background: linear-gradient(100deg, transparent 35%, rgba(255,255,255,0.08) 50%, transparent 65%); }
  @media (prefers-reduced-motion: reduce) { .pageSlot .placeholder::after { animation:none; } }
  .selectLayer { position:absolute; left:0; top:0; cursor:text; color:transparent; user-select:text;
                 pointer-events:auto; line-height: 1; transform-origin: 0 0; }
  .selectLayer span { position:absolute; white-space:pre; color:transparent; user-select:text;
                      -webkit-user-select:text; transform-origin: 0 0; }
  .selectLayer span::selection { background: rgba(43,108,223,0.4); color:transparent; }
  .highlightLayer { position:absolute; inset:0; pointer-events:none; }
  .drawLayer { position:absolute; inset:0; pointer-events:none; overflow:visible; }
  body.shapeMode .drawLayer { pointer-events:auto; cursor:crosshair; }
  body.shapeMode .selectLayer { pointer-events:none; }
  body.signMode .drawLayer { pointer-events:auto; cursor:crosshair; }
  body.signMode .selectLayer { pointer-events:none; }
  body.eraseMode .selectLayer { pointer-events:none; }
  body.eraseMode .pageSlot { cursor:cell; }
  .signMark.active { background:var(--accent-tint) !important; color:var(--accent-strong) !important; border-color:#bcd4f6 !important; }
  .sigCanvas { display:block; width:100%; height:160px; background:#fff;
               border:1px dashed var(--accent); border-radius:5px; cursor:crosshair; touch-action:none; }
  .sigHint { color:var(--text-3); font-size:0.82rem; margin:0.2rem 0; display:block; }
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
  .noteLayer { position:absolute; inset:0; pointer-events:none; }
  .redactLayer { position:absolute; inset:0; pointer-events:none; }
  .redactLayer .redactMark { position:absolute; z-index:5; background:rgba(0,0,0,0.55);
    border:1.5px solid var(--danger); box-sizing:border-box; }
  .redactLayer .redactMark button { position:absolute; top:-9px; right:-9px; width:18px; height:18px;
    padding:0; border-radius:50%; background:var(--danger); color:#fff; border:none; cursor:pointer;
    font-size:10px; line-height:18px; pointer-events:auto; }
  .viewer { position:relative; }
  .ocrBanner { position:sticky; top:0; z-index:40; width:fit-content; max-width:92%;
    margin:0 auto 0.8rem auto; display:flex; align-items:center; gap:0.6rem;
    background:var(--bg); color:var(--text); border:1px solid var(--border-2);
    border-radius:8px; padding:0.45rem 0.8rem; box-shadow:0 3px 14px rgba(0,0,0,0.25); }
  .ocrBanner.hidden { display:none; }
  .ocrBanner svg { width:18px; height:18px; color:var(--accent); flex:none; }
  .ocrBanner button:not(.propsClose) { background:var(--accent); color:#fff; border:none;
    border-radius:5px; padding:0.3rem 0.7rem; cursor:pointer; }
  .ocrBanner button:not(.propsClose):hover { background:var(--accent-strong); }
  /* Update hint (update.ts): small card above the status bar, closable,
     below dialogs (.propsOverlay is z-index 40). */
  .updateHint { position:fixed; right:14px; bottom:42px; z-index:35; width:340px; max-width:calc(100vw - 28px);
    display:flex; flex-direction:column; gap:0.35rem; font-size:0.85rem;
    background:var(--bg); color:var(--text); border:1px solid var(--border-2);
    border-radius:8px; padding:0.6rem 0.75rem; box-shadow:0 6px 24px rgba(0,0,0,0.28); }
  .updateHint.hidden { display:none; }
  .updateHintHead { display:flex; align-items:center; gap:0.45rem; }
  .updateHintHead svg { width:18px; height:18px; color:var(--accent); flex:none; }
  .updateHintHead strong { flex:1; }
  .updateHintSub { color:var(--text-3); font-size:0.78rem; }
  .updateHintNotes { color:var(--text-2); display:-webkit-box; -webkit-line-clamp:4; -webkit-box-orient:vertical; overflow:hidden; }
  .updateHintNotes:empty { display:none; }
  .updateHintActions { display:flex; gap:0.4rem; justify-content:flex-end; margin-top:0.2rem; }
  #updateDownloadBtn.hidden { display:none; }
  /* Welcome screen (no document open). */
  /* margin:auto on the card (not align-items:center) keeps it scrollable
     when the window is shorter than the card. */
  .welcome { position:absolute; inset:0; z-index:15; display:flex;
             background:var(--bg-2); overflow:auto; padding:1.5rem; box-sizing:border-box; }
  .welcome.hidden, body.merge .welcome, body.convert .welcome { display:none; }
  .welcomeCard { width:min(34rem, 100%); background:var(--bg); border:1px solid var(--border);
                 border-radius:12px; box-shadow:0 8px 30px rgba(20,30,50,0.10); padding:1.4rem 2rem 1.2rem; margin:auto;
                 display:flex; flex-direction:column; align-items:center; gap:0.6rem; box-sizing:border-box; }
  .welcomeLogo { width:120px; margin-bottom:0.2rem; }
  .welcome .primaryBtn { font-size:1rem; padding:0.6rem 1.4rem; display:inline-flex; align-items:center; gap:0.5rem; }
  .welcome .primaryBtn svg.lucide { width:18px; height:18px; }
  .welcomeHint { color:var(--text-3); font-size:0.85rem; }
  .welcomeActions { display:flex; gap:0.5rem; flex-wrap:wrap; justify-content:center; margin:0.5rem 0 0.3rem; }
  .welcomeActions button { display:inline-flex; align-items:center; gap:0.4rem; background:var(--bg-2);
                           color:var(--text); border:1px solid var(--border-2); border-radius:6px;
                           padding:0.4rem 0.75rem; cursor:pointer; font:inherit; font-size:0.85rem; }
  .welcomeActions button:hover { background:var(--hover); border-color:var(--accent); }
  .welcomeRecentTitle { align-self:stretch; font-size:0.75rem; text-transform:uppercase; letter-spacing:0.6px;
                        color:var(--text-3); border-top:1px solid var(--border); padding-top:0.8rem; margin-top:0.4rem; }
  .welcomeRecent { align-self:stretch; display:flex; flex-direction:column; gap:1px; }
  .welcomeRecent .recentItem { display:flex; flex-direction:column; text-align:left; background:transparent;
                               border:none; border-radius:6px; padding:0.35rem 0.5rem; cursor:pointer; font:inherit; color:var(--text); }
  .welcomeRecent .recentItem:hover { background:var(--hover); }
  .welcomeRecent .recentName { font-size:0.88rem; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
  .welcomeRecent .recentPath { font-size:0.72rem; color:var(--text-3); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
  .welcomeRecent .recentEmpty { color:var(--text-3); font-size:0.85rem; padding:0.3rem 0.5rem; }
  /* Commands that need an open document are inert until one is loaded. */
  body:not(.hasDoc) [data-doc] { opacity:0.4; pointer-events:none; }
  /* Primary action button (dialogs, panes, welcome). */
  button.primaryBtn { background:var(--accent) !important; border:1px solid var(--accent) !important; color:#fff !important;
                      border-radius:6px; cursor:pointer; font:inherit; }
  button.primaryBtn:hover:not(:disabled) { background:var(--accent-strong) !important; }
  button.primaryBtn:disabled { opacity:0.45; cursor:not-allowed; }
  body.theme-contrast button.primaryBtn { color:#000 !important; }
  /* Bottom status bar: message left, page navigation + zoom right. */
  .statusBar { display:flex; align-items:center; gap:0.35rem; padding:0 0.4rem; min-height:30px;
               background:var(--bg); border-top:1px solid var(--border); font-size:0.8rem; color:var(--text-2);
               user-select:none; -webkit-user-select:none; }
  .statusBar .status { flex:1; min-width:0; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; margin:0 0.3rem; }
  .sbGroup { display:inline-flex; align-items:center; gap:0.15rem; }
  .sbSep { width:1px; height:18px; background:var(--border-2); margin:0 0.3rem; }
  .sbBtn { background:transparent; border:1px solid transparent; border-radius:4px; color:var(--text-2);
           width:26px; height:24px; padding:0; display:inline-flex; align-items:center; justify-content:center; cursor:pointer; }
  .sbBtn:hover:not(:disabled) { background:var(--hover); color:var(--text); }
  .sbBtn.active { background:var(--accent-tint); color:var(--accent-strong); }
  .statusBar input, .statusBar select { background:var(--bg); color:var(--text); border:1px solid var(--border-2);
           border-radius:4px; font:inherit; font-size:0.8rem; padding:0.1rem 0.25rem; }
  .statusBar #pageInput { width:3.2rem; text-align:center; }
  .statusBar #pageTotal { padding:0 0.2rem; font-variant-numeric:tabular-nums; }
  .statusBar #zoomSelect { width:7.2rem; }
  .splitIndent { margin-left:1.5rem !important; }
  .numSm { width:3.5rem; }
  .hiddenList { display:flex; flex-direction:column; gap:2px; font-size:0.85rem; color:var(--text-2); }
  .hiddenList div { white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
  /* Text editing: click targets on the page + the floating editor. */
  .textEditLayer { position:absolute; inset:0; pointer-events:none; }
  .textBoxHs, .textBlockHs { position:absolute; pointer-events:auto; cursor:text; border-radius:2px; }
  .textBoxHs:hover { outline:1.5px solid var(--accent); outline-offset:1px; }
  .textBlockHs { outline:1px dashed rgba(43,108,223,0.45); outline-offset:1px; }
  .textBlockHs:hover { outline:1.5px solid var(--accent); background:rgba(43,108,223,0.10); }
  body.shapeMode .textBoxHs, body.annotMode .textBoxHs, body.signMode .textBoxHs,
  body.eraseMode .textBoxHs, body.wheelZoomMode .textBoxHs { pointer-events:none; }
  body.textEditMode .textBoxHs { pointer-events:auto; }
  body.textEditMode .drawLayer { cursor:text; }
  .textEdPatch { position:absolute; z-index:19; pointer-events:none; }
  .textEd { position:absolute; z-index:20; transform-origin:0 0; }
  .textEdBox { display:block; box-sizing:content-box; min-width:1.2em; min-height:1.2em; line-height:1.2;
               white-space:pre; outline:1.5px solid var(--accent); outline-offset:2px; border-radius:1px;
               box-shadow:0 0 0 5px rgba(43,108,223,0.14); cursor:text; caret-color:var(--accent);
               user-select:text; -webkit-user-select:text; }
  .textEdBar { position:absolute; z-index:21; display:flex; align-items:center; gap:2px; padding:4px;
               background:var(--bg); color:var(--text); border:1px solid var(--border-2); border-radius:10px;
               box-shadow:0 6px 20px rgba(0,0,0,0.22), 0 1px 3px rgba(0,0,0,0.12);
               font:12px "Segoe UI", system-ui, sans-serif; white-space:nowrap; user-select:none; }
  .textEdBar select, .textEdBar .fmtSize { height:26px; border:1px solid var(--border); border-radius:6px;
               background:var(--bg-2); color:var(--text); font:inherit; padding:0 4px; }
  .textEdBar .fmtFamily { max-width:9.5rem; }
  .textEdBar .fmtSize { width:3.4rem; }
  .textEdBar .teBtn { position:relative; display:inline-flex; align-items:center; justify-content:center;
               width:26px; height:26px; padding:0; border:none; border-radius:6px; background:transparent;
               color:var(--text); cursor:pointer; }
  .textEdBar .teBtn:hover:not(:disabled) { background:var(--hover); }
  .textEdBar .teBtn:disabled { opacity:0.35; cursor:default; }
  .textEdBar .teBtn.active { background:var(--accent-tint); color:var(--accent-strong); }
  .textEdBar .teBtn svg.lucide { width:15px; height:15px; }
  /* Color buttons (text bar and ribbon): icon over a bar showing the color. */
  .fmtSwatchBtn svg.lucide { width:13px !important; height:13px !important; margin-bottom:5px; }
  .fmtSwatch { position:absolute; left:5px; right:5px; bottom:4px; height:4px; border-radius:2px;
               border:1px solid var(--border-2); }
  .fmtSwatch.none, .colorBtnSwatch.none, .cpSwatch.none {
    background:linear-gradient(to top right, transparent calc(50% - 1px), #d23b3b calc(50% - 1px), #d23b3b calc(50% + 1px), transparent calc(50% + 1px)),
               repeating-conic-gradient(var(--bg-3) 0 25%, var(--bg) 0 50%) 0 0 / 8px 8px !important; }
  /* Text group of the Edit tab: two rows (font + size; style + colors). */
  .textFmt { display:flex; flex-direction:column; justify-content:center; gap:4px; height:3.5rem;
             padding:0 0.35rem; box-sizing:border-box; }
  .textFmt .tfRow { display:flex; align-items:center; gap:3px; }
  .ribbonBody .textFmt select, .ribbonBody .textFmt input { height:1.55rem; box-sizing:border-box;
             padding:0 0.3rem; font-size:0.78rem; }
  .textFmt .fmtFamily { width:9.5rem; }
  .textFmt .fmtSize { width:3.4rem; }
  .ribbonBody .textFmt .tfBtn { position:relative; width:1.75rem; height:1.55rem; padding:0;
             background:transparent; border:1px solid transparent; border-radius:4px; }
  .ribbonBody .textFmt .tfBtn svg.lucide { width:15px; height:15px; }
  .ribbonBody .textFmt .tfBtn.active { background:var(--accent-tint); color:var(--accent-strong); border-color:#bcd4f6; }
  /* Swatch button replacing <input type=color> (colorpicker.ts). */
  .ribbonBody .colorBtn, .colorBtn { display:inline-flex; align-items:center; gap:2px; height:1.6rem;
             padding:0 0.2rem 0 0.25rem; background:var(--bg); border:1px solid var(--border-2);
             border-radius:4px; cursor:pointer; color:var(--text-2); }
  .colorBtn .colorBtnSwatch { display:block; width:1.45rem; height:1rem; border-radius:3px;
             border:1px solid rgba(0,0,0,0.25); box-sizing:border-box; }
  .colorBtn svg.lucide { width:11px; height:11px; }
  /* Color popover: palette, recent colors, custom picker. */
  .colorPop { position:fixed; z-index:1000; width:248px; padding:8px; box-sizing:border-box;
              background:var(--bg); color:var(--text); border:1px solid var(--border-2); border-radius:8px;
              box-shadow:0 8px 24px rgba(20,30,50,0.22); font:12px "Segoe UI", system-ui, sans-serif;
              display:flex; flex-direction:column; gap:6px; user-select:none; }
  .colorPop.hidden { display:none; }
  .colorPop .hidden { display:none !important; }
  .cpGrid, .cpRecentRow { display:grid; grid-template-columns:repeat(8, 1fr); gap:4px; }
  .cpSwatch { width:22px; height:22px; padding:0; border-radius:4px; cursor:pointer;
              background:var(--c) !important; border:1px solid rgba(0,0,0,0.22) !important; box-sizing:border-box; }
  .cpSwatch:hover { transform:scale(1.12); box-shadow:0 1px 4px rgba(0,0,0,0.3); }
  .cpSwatch.current { outline:2px solid var(--accent); outline-offset:1px; }
  .cpLabel { color:var(--text-3); font-size:11px; margin-bottom:3px; }
  .cpNone, .cpMoreToggle { display:flex; align-items:center; gap:6px; padding:3px 4px; border:none;
              border-radius:4px; background:transparent; color:var(--text); cursor:pointer; font:inherit; text-align:left; }
  .cpNone:hover, .cpMoreToggle:hover { background:var(--hover); }
  .cpNone .cpSwatch { width:18px; height:18px; display:inline-block; }
  .cpNone.current { color:var(--accent-strong); font-weight:600; }
  .cpMoreToggle { border-top:1px solid var(--border); border-radius:0; padding-top:6px; }
  .cpMoreToggle svg.lucide { width:14px; height:14px; transition:transform .12s ease; }
  .cpMoreToggle.open svg.lucide { transform:rotate(90deg); }
  .cpCustom { display:flex; flex-direction:column; gap:6px; }
  .cpSV { position:relative; height:120px; border-radius:4px; cursor:crosshair; touch-action:none;
          background:linear-gradient(to top, #000, transparent), linear-gradient(to right, #fff, var(--hue, #f00)); }
  .cpSVThumb { position:absolute; width:12px; height:12px; margin:-7px 0 0 -7px; border-radius:50%;
               border:2px solid #fff; box-shadow:0 0 0 1px rgba(0,0,0,0.5); pointer-events:none; }
  .cpHue { -webkit-appearance:none; appearance:none; width:100%; height:12px; margin:0; border-radius:6px;
           background:linear-gradient(to right, #f00, #ff0, #0f0, #0ff, #00f, #f0f, #f00); cursor:pointer; }
  .cpHue::-webkit-slider-thumb { -webkit-appearance:none; width:14px; height:14px; border-radius:50%;
           background:#fff; border:1px solid rgba(0,0,0,0.45); box-shadow:0 1px 3px rgba(0,0,0,0.3); }
  .cpFields { display:flex; align-items:center; justify-content:space-between; gap:3px; }
  .cpFields label { display:inline-flex; align-items:center; gap:2px; color:var(--text-2); font-size:11px; }
  .cpFields input { width:2rem; min-width:0; height:22px; box-sizing:border-box; padding:0 2px; font:inherit;
                    background:var(--bg-2); color:var(--text); border:1px solid var(--border); border-radius:4px; }
  .cpFields .cpHex { width:3.6rem; font-family:Consolas, monospace; }
  .cpFields input[type=number]::-webkit-inner-spin-button { display:none; }
  .cpPreview { width:22px; height:22px; flex:none; border-radius:4px; border:1px solid rgba(0,0,0,0.22); }
  .cpActions { display:flex; justify-content:flex-end; }
  .cpActions .cpOk { padding:3px 14px; border-radius:4px; cursor:pointer; font:inherit; }
  /* Shapes menu: grouped two-column grid. */
  .shapePop { min-width:260px; }
  .shapePop .popHead { font-size:0.7rem; color:var(--text-3); text-transform:uppercase; letter-spacing:0.4px;
                       padding:0.35rem 0.5rem 0.15rem; }
  .shapePop .shapeGrid { display:grid; grid-template-columns:1fr 1fr; gap:1px; }
  .shapePop .mi.active { background:var(--accent-tint); color:var(--accent-strong); }
  #shapeMenuBtn.active { background:var(--accent-tint); color:var(--accent-strong); border-color:#bcd4f6; }
  .textEdBar .teSep { width:1px; height:18px; margin:0 3px; background:var(--border); }
  .textEdBar .teGrip { display:inline-flex; align-items:center; width:16px; height:26px; color:var(--text-3);
               cursor:grab; touch-action:none; }
  .textEdBar .teGrip:active { cursor:grabbing; }
  .textEdBar .teGrip svg.lucide { width:15px; height:15px; }
  .textEdBar .teDelete:hover { background:rgba(210,59,59,0.14); color:var(--danger); }
  .textEdBar .teDone { background:var(--accent); color:#fff; }
  .textEdBar .teDone:hover { background:var(--accent-strong); }
  .noteLayer .noteHotspot { position:absolute; pointer-events:auto; cursor:pointer; border-radius:3px; }
  .noteLayer .noteHotspot:hover { outline:1px solid rgba(179,154,0,0.7); }
  .stickyNote { position:absolute; width:220px; z-index:20; background:#fff8b0; color:#000;
                border:1px solid #b39a00; box-shadow:2px 2px 6px rgba(0,0,0,0.35);
                font:13px "Segoe UI", Helvetica, Arial, sans-serif; pointer-events:auto; }
  .stickyNote.hidden { display:none; }
  .stickyNoteHead { padding:2px 6px; background:#e6cf00; border-bottom:1px solid #b39a00;
                    font-size:12px; display:flex; gap:0.4rem; align-items:baseline; }
  .stickyNoteHead .meta { margin-left:auto; font-size:10px; color:#5a4d00; white-space:nowrap;
                          overflow:hidden; text-overflow:ellipsis; max-width:60%; }
  .stickyNoteBody { padding:4px 6px; white-space:pre-wrap; overflow-wrap:break-word;
                    max-height:160px; overflow:auto; }
  .stickyNote textarea { display:block; width:100%; height:80px; box-sizing:border-box; border:0;
                         background:transparent; color:#000; font:inherit; padding:4px 6px; resize:vertical; }
  .linkLayer a { position:absolute; pointer-events:auto; display:block;
                 background: transparent; border: 0; cursor:pointer;
                 text-decoration:none; }
  .linkLayer a:hover { background: rgba(43,108,223,0.18); outline: 1px solid rgba(43,108,223,0.55); }
  /* 3D annotations: badge + preview while inactive, live WebGL view when active. */
  .threeDLayer { position:absolute; inset:0; pointer-events:none; }
  .threeDBox { position:absolute; pointer-events:auto; cursor:pointer; }
  .threeDBox:hover { outline:1px dashed rgba(43,108,223,0.65); }
  .threeDBox.active { cursor:default; outline:2px solid var(--accent); z-index:2; }
  .threeDBox .threeDPreview { position:absolute; inset:0; width:100%; height:100%; pointer-events:none; }
  body.dark .threeDBox .threeDPreview { filter: invert(0.92) hue-rotate(180deg); }
  .threeDBadge { position:absolute; left:6px; top:6px; display:flex; align-items:center; gap:0.3rem;
    background:rgba(255,255,255,0.94); color:#1f2937; border:1px solid rgba(43,108,223,0.6); border-radius:6px;
    padding:0.15rem 0.45rem; font:600 0.75rem "Segoe UI", system-ui, sans-serif; cursor:pointer;
    box-shadow:0 1px 4px rgba(0,0,0,0.18); }
  .threeDBadge svg { width:14px; height:14px; color:#2b6cdf; }
  .threeDBadge:hover { background:#fff; border-color:#2b6cdf; }
  .threeDView { position:absolute; inset:0; overflow:hidden; background:#fff; }
  .threeDView [hidden] { display:none !important; }
  .threeDView canvas { display:block; width:100%; height:100%; touch-action:none; }
  .threeDBar { position:absolute; left:4px; right:4px; top:4px; display:flex; flex-wrap:wrap; gap:3px; align-items:center; }
  .threeDBar select, .threeDBar button { font:0.75rem "Segoe UI", system-ui, sans-serif; background:rgba(255,255,255,0.94);
    color:#1f2937; border:1px solid #c9ced6; border-radius:5px; height:24px; }
  .threeDBar select { max-width:9.5rem; padding:0 0.2rem; }
  .threeDBar button { width:26px; display:inline-flex; align-items:center; justify-content:center; padding:0; cursor:pointer; }
  .threeDBar button:hover, .threeDSection button:hover { background:#e8f0fd; border-color:#2b6cdf; }
  .threeDBar button svg, .threeDSection button svg { width:15px; height:15px; }
  .threeDTree { position:absolute; left:4px; top:34px; bottom:4px; width:min(220px, 40%); overflow:auto;
    background:rgba(255,255,255,0.95); border:1px solid #c9ced6; border-radius:6px; color:#1f2937;
    font:0.75rem "Segoe UI", system-ui, sans-serif; padding:0.3rem 0.4rem; }
  .threeDTree ul { list-style:none; margin:0; padding-left:0.9rem; }
  .threeDTree > ul { padding-left:0; }
  .threeDTree label { display:flex; gap:0.3rem; align-items:center; white-space:nowrap; cursor:pointer; }
  .threeDTreeTitle { font-weight:600; margin-bottom:0.2rem; }
  .threeDTree summary { cursor:pointer; list-style-position:outside; margin-left:0.8rem; }
  .threeDTree summary label { display:inline-flex; }
  .threeDTree li > label { margin-left:0.8rem; }
  .threeDSection { position:absolute; left:50%; transform:translateX(-50%); bottom:6px; display:flex; gap:4px; align-items:center;
    background:rgba(255,255,255,0.95); border:1px solid #c9ced6; border-radius:6px; padding:3px 6px; }
  .threeDSection select, .threeDSection button { height:22px; border:1px solid #c9ced6; border-radius:4px; background:#fff; color:#1f2937; }
  .threeDSection button { width:24px; padding:0; display:inline-flex; align-items:center; justify-content:center; cursor:pointer; }
  .threeDSection input[type=range] { width:min(12rem, 40vw); }
  .threeDHint { position:absolute; left:6px; top:36px; max-width:min(19rem, calc(100% - 12px)); cursor:default;
    background:var(--bg); color:var(--text); border:1px solid var(--border-2); border-radius:8px; padding:0.5rem 0.6rem;
    font-size:0.8rem; box-shadow:0 4px 16px rgba(0,0,0,0.2); }
  .threeDHint p { margin:0 0 0.45rem; }
  .threeDHint button { background:var(--bg); color:var(--text); border:1px solid var(--border-2); border-radius:5px; padding:0.25rem 0.7rem; cursor:pointer; }
  .threeDFullscreen { position:fixed; inset:0; z-index:60; background:#fff; }
  .threeDImport .propsModal { min-width:auto; }
  .threeDImportView { width:520px; max-width:80vw; height:320px; border:1px solid var(--border-2); border-radius:6px; overflow:hidden; background:#fff; }
  .threeDImportView canvas { display:block; max-width:100%; }
  .threeDImportOpts { display:flex; gap:1.2rem; }
  .threeDImportOpts label { flex-direction:row !important; align-items:center; gap:0.4rem !important; }
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
  .searchPanel { position:absolute; top: 0.5rem; right: 1rem; background:var(--bg);
                 border:1px solid var(--border-2); border-radius:6px; padding: 0.35rem;
                 box-shadow:0 6px 20px rgba(20,30,50,0.18);
                 display:flex; gap:0.3rem; align-items:center; z-index: 20; }
  .searchPanel.hidden { display:none; }
  .searchPanel input[type=text], .searchPanel #searchInput {
    background:var(--bg); color:var(--text); border:1px solid var(--border-2); border-radius:4px; padding:0.2rem 0.4rem; width:16rem;
  }
  .searchPanel button { background:var(--bg); color:var(--text); border:1px solid var(--border-2); border-radius:4px; padding:0.2rem 0.4rem; cursor:pointer; display:inline-flex; align-items:center; }
  .searchPanel button:hover { background:var(--hover); }
  .searchPanel label { display:inline-flex; align-items:center; gap:0.2rem; font-size:0.82rem; color:var(--text-2); }
  .searchPanel #searchCount { font-size:0.8rem; color:var(--text-2); min-width:4.5rem; font-variant-numeric:tabular-nums; }
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
  body.presentation .ribbon, body.presentation .sidebar, body.presentation .statusBar { display:none; }
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
  body.organize .thumb { cursor:grab; transition:opacity 0.12s, transform 0.12s; }
  body.organize .thumb:hover:not(.selected) .thumbImg { box-shadow:0 0 0 2px var(--border-2), 0 1px 4px rgba(20,30,50,0.25); }
  body.organize .thumb:active, body.pageDragging, body.pageDragging * { cursor:grabbing !important; }
  /* Pages being moved: faded + dashed outline, so it's clear what moves. */
  body.organize .thumb.dragging { opacity:0.3; transform:scale(0.96); outline:2px dashed var(--accent); outline-offset:2px; }
  /* Insertion marker: a bright bar in the gap before/after the target page.
     (An inset shadow on the thumb would be hidden under the page image.) */
  body.organize .thumb.dropBefore::before, body.organize .thumb.dropAfter::after,
  .mergeTile.dropBefore::before, .mergeTile.dropAfter::after {
    content:""; position:absolute; top:-8px; bottom:-8px; width:4px; border-radius:2px; z-index:6;
    background:var(--accent); box-shadow:0 0 0 3px var(--accent-tint), 0 0 12px var(--accent);
    pointer-events:none;
  }
  body.organize .thumb.dropBefore::before, .mergeTile.dropBefore::before { left:calc(-0.5rem - 4px); }
  body.organize .thumb.dropAfter::after, .mergeTile.dropAfter::after { right:calc(-0.5rem - 4px); }
  body.organize .thumbLabel { font-size:0.78rem; padding:2px 7px; }
  .dragPreview {
    position:fixed; pointer-events:none; z-index:9999;
    background:var(--accent); color:#fff; padding:4px 10px; border-radius:999px;
    font-size:0.82rem; box-shadow:0 3px 10px rgba(20,30,50,0.35);
  }
  /* Page drag ghost: small picture of the grabbed page + "move N pages". */
  .dragPreview.pageGhost { display:flex; flex-direction:column; align-items:center; gap:4px;
    padding:4px; border-radius:6px; background:var(--bg); color:var(--text);
    border:2px solid var(--accent); transform:rotate(-3deg); }
  .dragPreview.pageGhost img { width:72px; height:auto; display:block; background:#fff;
    box-shadow:0 1px 3px rgba(0,0,0,0.25); }
  .dragPreview.pageGhost span { background:var(--accent); color:#fff; border-radius:999px;
    padding:1px 8px; font-size:0.75rem; white-space:nowrap; }
  body.theme-contrast .dragPreview.pageGhost span { color:#000; }
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
  .mergeHeader button { display:inline-flex; align-items:center; gap:0.35rem; }
  .mergeHint { color:var(--text-3); font-size:0.85rem; flex:1; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
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
  /* Settings modal: vertical nav rail + scrollable body (no horizontal scroll). */
  .settingsModal { width: 46rem; max-width: 92vw; }
  .settingsLayout { display:flex; flex:1; min-height:0; }
  .settingsNav { display:flex; flex-direction:column; gap:0.1rem; padding:0.6rem 0.4rem; border-right:1px solid var(--border); background:var(--bg-2); min-width:11rem; flex-shrink:0; }
  .settingsNavItem { text-align:left; background:transparent; color:var(--text-2); border:none; border-radius:6px; padding:0.45rem 0.7rem; cursor:pointer; font:inherit; font-size:0.88rem; }
  .settingsNavItem:hover { background:var(--hover); color:var(--text); }
  .settingsNavItem.active { background:var(--accent-tint); color:var(--accent); font-weight:600; }
  .settingsBody { flex:1; min-width:0; overflow-y:auto; overflow-x:hidden; }
  .settingsSection { padding:0.8rem 1rem; scroll-margin-top:0.4rem; }
  .settingsSection + .settingsSection { border-top:1px solid var(--border); }
  .settingsSection label.settingsCheck { flex-direction:row; align-items:center; gap:0.5rem; cursor:pointer; margin:0.3rem 0; }
  .settingsSection label.settingsCheck input { width:auto; }
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
