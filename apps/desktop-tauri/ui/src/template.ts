import { t } from "./i18n";
import appIcon from "./assets/logo.svg";
import logoText from "./assets/logo-text.svg";

export function buildHtml(): string {
  return `
<main class="root">
  <header class="ribbon">
    <div class="titleBar" data-tauri-drag-region>
      <span class="appGlyph" data-tauri-drag-region><img src="${appIcon}" alt="" draggable="false" /></span>
      <div class="qat">
        <button id="sidebarBtn" title="${t("tip.sidebar")}"><i data-lucide="panel-left"></i></button>
        <button id="commentToggleBtn" title="${t("tip.comments")}"><i data-lucide="message-square-text"></i></button>
        <span class="sep"></span>
        <button id="open" title="${t("tip.open")}"><i data-lucide="folder-open"></i></button>
        <button id="recentBtn" title="${t("tip.recent")}" class="hasCaret"><i data-lucide="history"></i></button>
        <div id="recentMenu" class="menu hidden"></div>
        <button id="saveAsBtn" title="${t("tip.saveAs")}"><i data-lucide="save"></i></button>
        <button id="printBtn" title="${t("tip.print")}"><i data-lucide="printer"></i></button>
        <span class="sep"></span>
        <button id="undoBtn" title="${t("tip.undo")}" disabled><i data-lucide="undo-2"></i></button>
        <button id="redoBtn" title="${t("tip.redo")}" disabled><i data-lucide="redo-2"></i></button>
        <span class="sep"></span>
        <button id="prevPage" title="${t("tip.prevPage")}"><i data-lucide="chevron-left"></i></button>
        <input id="pageInput" type="number" min="1" value="1" style="width:3.4rem" />
        <span id="pageTotal">/ 0</span>
        <button id="nextPage" title="${t("tip.nextPage")}"><i data-lucide="chevron-right"></i></button>
        <button id="gotoBtn" title="${t("tip.goto")}"><i data-lucide="arrow-right-to-line"></i></button>
      </div>
      <div class="titleDrag" data-tauri-drag-region><span id="titlebarTitle">Docura PDF</span></div>
      <button id="settingsBtnTop" class="winBtn" title="${t("tip.settings")}"><i data-lucide="settings"></i></button>
      <div class="winControls">
        <button id="winMin" title="${t("tip.winMin")}" tabindex="-1"><i data-lucide="minus"></i></button>
        <button id="winMax" title="${t("tip.winMax")}" tabindex="-1"><i data-lucide="square"></i></button>
        <button id="winClose" title="${t("tip.winClose")}" tabindex="-1"><i data-lucide="x"></i></button>
      </div>
    </div>
    <div class="tabStrip">
      <button class="ribbonTab active" data-ribbon-tab="reader">${t("tab.reader")}</button>
      <button class="ribbonTab" data-ribbon-tab="comment">${t("tab.comment")}</button>
      <button class="ribbonTab" data-ribbon-tab="view">${t("tab.view")}</button>
      <button class="ribbonTab" data-ribbon-tab="edit">${t("tab.edit")}</button>
      <button class="ribbonTab" data-ribbon-tab="forms">${t("tab.forms")}</button>
      <button class="ribbonTab" data-ribbon-tab="sign">${t("tab.sign")}</button>
      <button class="ribbonTab" data-ribbon-tab="convert">${t("tab.convert")}</button>
      <button class="ribbonTab" data-ribbon-tab="organize">${t("tab.organize")}</button>
      <button class="ribbonTab" data-ribbon-tab="protect">${t("tab.protect")}</button>
    </div>
    <div class="ribbonBody" data-active="reader">
      <div data-ribbon="reader" class="ribbonGroup">
        <div class="groupBody">
          <button class="cmd" id="zoomOut" title="${t("tip.zoomOut")}"><i data-lucide="zoom-out"></i><span>${t("lbl.zoomOut")}</span></button>
          <div class="fieldCol">
            <select id="fitSelect" title="${t("tip.fit")}">
              <option value="custom">${t("fit.custom")}</option>
              <option value="width">${t("fit.width")}</option>
              <option value="page">${t("fit.page")}</option>
              <option value="actual">${t("fit.actual")}</option>
            </select>
            <select id="zoomSelect" title="${t("tip.zoom")}">
              <option value="custom" id="zoomCustomOpt">100%</option>
              <option value="0.1">10%</option>
              <option value="0.5">50%</option>
              <option value="0.8">80%</option>
              <option value="1">100%</option>
              <option value="1.2">120%</option>
              <option value="1.5">150%</option>
              <option value="2">200%</option>
            </select>
          </div>
          <button class="cmd" id="zoomIn" title="${t("tip.zoomIn")}"><i data-lucide="zoom-in"></i><span>${t("lbl.zoomIn")}</span></button>
          <button class="cmd" id="wheelZoomBtn" title="${t("tip.wheelZoom")}"><i data-lucide="mouse"></i><span>${t("lbl.pan")}</span></button>
        </div>
      </div>
      <div data-ribbon="reader" class="ribbonGroup">
        <div class="groupBody">
          <button class="cmd" id="searchBtn" title="${t("tip.search")}"><i data-lucide="search"></i><span>${t("lbl.search")}</span></button>
          <button class="cmd" id="propsBtn" title="${t("tip.props")}"><i data-lucide="info"></i><span>${t("lbl.props")}</span></button>
        </div>
      </div>
      <div data-ribbon="view" class="ribbonGroup">
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
            <button class="cmd menuTrigger" id="viewRotateBtn" aria-haspopup="true" aria-expanded="false" title="${t("grp.rotate")}"><i data-lucide="rotate-cw-square"></i><span>${t("lbl.rotate")}</span></button>
            <div class="popover hidden" role="menu">
              <button class="mi" id="rotLeft" role="menuitem" title="${t("tip.rotLeft")}"><i data-lucide="rotate-ccw"></i><span>${t("tip.rotLeft")}</span></button>
              <button class="mi" id="rotRight" role="menuitem" title="${t("tip.rotRight")}"><i data-lucide="rotate-cw"></i><span>${t("tip.rotRight")}</span></button>
            </div>
          </div>
        </div>
      </div>
      <div data-ribbon="view" class="ribbonGroup">
        <div class="groupBody">
          <button class="cmd" id="darkToggle" title="${t("tip.invert")}"><i data-lucide="moon"></i><span>${t("lbl.invert")}</span></button>
          <button class="cmd" id="fullscreenBtn" title="${t("tip.fullscreen")}"><i data-lucide="maximize"></i><span>${t("lbl.fullscreen")}</span></button>
          <button class="cmd" id="presentBtn" title="${t("tip.present")}"><i data-lucide="presentation"></i><span>${t("lbl.present")}</span></button>
        </div>
      </div>
      <div data-ribbon="edit" class="ribbonGroup">
        <div class="groupBody">
          <button class="cmd modeBtn active" data-mode="select" title="${t("tip.modeSelect")}"><i data-lucide="mouse-pointer-2"></i><span>${t("lbl.select")}</span></button>
          <button class="cmd modeBtn" data-mode="delete" title="${t("tip.modeDelete")}"><i data-lucide="trash-2"></i><span>${t("lbl.delMode")}</span></button>
          <button class="cmd deleteAction" id="deleteSelectionBtn" disabled title="${t("tip.deleteSel")}"><i data-lucide="trash-2"></i><span>${t("lbl.delPage")}</span></button>
          <button class="cmd" id="extractSelectionBtn" disabled title="${t("tip.extractSel")}"><i data-lucide="file-output"></i><span>${t("lbl.extract")}</span></button>
        </div>
      </div>
      <div data-ribbon="edit" class="ribbonGroup">
        <div class="groupBody">
          <div class="menuHost">
            <button id="rotateMenuBtn" class="cmd menuTrigger" aria-haspopup="true" aria-expanded="false" title="${t("tip.rotatePages")}"><i data-lucide="rotate-cw-square"></i><span>${t("lbl.rotate")}</span></button>
            <div id="rotateMenu" class="popover hidden" role="menu">
              <button id="rotateLeftBtn" class="mi" role="menuitem" title="${t("tip.rotateCcw")}"><i data-lucide="rotate-ccw"></i><span>${t("tip.rotateCcw")}</span></button>
              <button id="rotateRightBtn" class="mi" role="menuitem" title="${t("tip.rotateCw")}"><i data-lucide="rotate-cw"></i><span>${t("tip.rotateCw")}</span></button>
              <button id="rotate180Btn" class="mi" role="menuitem" title="${t("tip.rotate180")}"><i data-lucide="arrow-up-down"></i><span>${t("tip.rotate180")}</span></button>
            </div>
          </div>
          <div class="menuHost">
            <button id="insertMenuBtn" class="cmd menuTrigger" aria-haspopup="true" aria-expanded="false" title="${t("tip.insertPages")}"><i data-lucide="file-plus-2"></i><span>${t("lbl.insert")}</span></button>
            <div id="insertMenu" class="popover hidden" role="menu">
              <button id="insertBeforeBtn" class="mi" role="menuitem" title="${t("tip.insertBefore")}"><i data-lucide="arrow-up-to-line"></i><span>${t("tip.insertBefore")}</span></button>
              <button id="insertAfterBtn" class="mi" role="menuitem" title="${t("tip.insertAfter")}"><i data-lucide="arrow-down-to-line"></i><span>${t("tip.insertAfter")}</span></button>
              <button id="insertEndBtn" class="mi" role="menuitem" title="${t("tip.insertEnd")}"><i data-lucide="list-end"></i><span>${t("tip.insertEnd")}</span></button>
            </div>
          </div>
        </div>
      </div>
      <span data-ribbon="edit" id="modeHint" class="modeHint"></span>
      <div data-ribbon="organize" class="ribbonGroup">
        <div class="groupBody">
          <button class="cmd" id="mergePdfsBtn" title="${t("tip.merge")}"><i data-lucide="merge"></i><span>${t("lbl.merge")}</span></button>
          <button class="cmd" id="extractSelectionOrgBtn" disabled title="${t("tip.extractSel")}"><i data-lucide="file-output"></i><span>${t("lbl.extract")}</span></button>
          <button class="cmd" id="splitPdfBtn" title="${t("tip.split")}"><i data-lucide="scissors"></i><span>${t("lbl.split")}</span></button>
        </div>
      </div>
      <div data-ribbon="comment" class="ribbonGroup">
        <div class="groupBody" id="annotToolGroup">
          <button class="cmd annotTool" data-annot="highlight" title="${t("tip.highlight")}"><i data-lucide="highlighter"></i><span>${t("lbl.highlight")}</span></button>
          <button class="cmd annotTool" data-annot="underline" title="${t("tip.underline")}"><i data-lucide="underline"></i><span>${t("lbl.underline")}</span></button>
          <button class="cmd annotTool" data-annot="strikeout" title="${t("tip.strikeout")}"><i data-lucide="strikethrough"></i><span>${t("lbl.strikeout")}</span></button>
          <button class="cmd annotTool" data-annot="squiggly" title="${t("tip.squiggly")}"><i data-lucide="waves"></i><span>${t("lbl.squiggly")}</span></button>
          <label class="fieldCol" title="${t("tip.annotColor")}">
            <input type="color" id="annotColor" value="#ffe600" />
            <span class="fieldCaption"><i data-lucide="palette"></i>${t("lbl.color")}</span>
          </label>
        </div>
      </div>
      <div data-ribbon="comment" class="ribbonGroup">
        <div class="groupBody">
          <div class="menuHost">
            <button class="cmd menuTrigger" id="shapeMenuBtn" aria-haspopup="true" aria-expanded="false" title="${t("tip.drawDrag")}"><i data-lucide="shapes"></i><span>${t("lbl.draw")}</span></button>
            <div class="popover hidden" role="menu" id="shapeToolGroup">
              <button class="mi shapeTool" data-shape="rect" role="menuitem" title="${t("tip.rect")}"><i data-lucide="square"></i><span>${t("tip.rect")}</span></button>
              <button class="mi shapeTool" data-shape="circle" role="menuitem" title="${t("tip.circle")}"><i data-lucide="circle"></i><span>${t("tip.circle")}</span></button>
              <button class="mi shapeTool" data-shape="line" role="menuitem" title="${t("tip.line")}"><i data-lucide="slash"></i><span>${t("tip.line")}</span></button>
              <button class="mi shapeTool" data-shape="arrow" role="menuitem" title="${t("tip.arrow")}"><i data-lucide="arrow-up-right"></i><span>${t("tip.arrow")}</span></button>
            </div>
          </div>
          <button class="cmd shapeTool" data-shape="ink" title="${t("tip.ink")}"><i data-lucide="pen-line"></i><span>${t("lbl.ink")}</span></button>
          <button class="cmd shapeTool" data-shape="note" title="${t("tip.note")}"><i data-lucide="sticky-note"></i><span>${t("lbl.note")}</span></button>
          <button class="cmd shapeTool" data-shape="stamp" title="${t("tip.stamp")}"><i data-lucide="stamp"></i><span>${t("lbl.stamp")}</span></button>
        </div>
      </div>
      <div data-ribbon="comment" class="ribbonGroup">
        <div class="groupBody">
          <button class="cmd shapeTool" data-shape="freetext" title="${t("tip.freetext")}"><i data-lucide="type"></i><span>${t("lbl.textbox")}</span></button>
          <div class="menuHost">
            <button class="cmd menuTrigger" id="textOptsBtn" aria-haspopup="true" aria-expanded="false" title="${t("grp.text")}"><i data-lucide="sliders-horizontal"></i><span>${t("grp.text")}</span></button>
            <div class="popover hidden popoverForm" role="menu">
              <label class="popRow" title="${t("tip.textColor")}"><span>${t("tip.textColor")}</span><input type="color" id="textFgColor" value="#000000" /></label>
              <label class="popRow" title="${t("tip.bgColor")}"><span>${t("tip.bgColor")}</span><input type="color" id="textBgColor" value="#ffffff" /></label>
              <label class="popRow" title="${t("tip.transparentBg")}"><span>${t("tip.transparentBg")}</span><input type="checkbox" id="textBgTransparent" /></label>
              <label class="popRow" title="${t("tip.fontFamily")}"><span>${t("tip.fontFamily")}</span>
                <select id="textFontFamily"><option value="Helv" selected>Helvetica</option></select></label>
              <label class="popRow" title="${t("tip.fontSize")}"><span>${t("tip.fontSize")}</span>
                <input id="textFontSize" type="number" min="6" max="96" step="1" value="14" style="width:3.5rem;" /></label>
            </div>
          </div>
        </div>
      </div>
      <div data-ribbon="comment" class="ribbonGroup">
        <div class="groupBody">
          <button class="cmd shapeTool" data-shape="region" title="${t("tip.regionTool")}"><i data-lucide="crop"></i><span>${t("lbl.region")}</span></button>
          <button class="cmd" id="eraseAnnotBtn" title="${t("tip.eraseAnnot")}"><i data-lucide="eraser"></i><span>${t("lbl.erase")}</span></button>
          <button class="cmd active" id="printToggleBtn" title="${t("tip.printToggle")}"><i data-lucide="printer"></i><span>${t("lbl.print")}</span></button>
        </div>
      </div>
      <div data-ribbon="edit" class="ribbonGroup">
        <div class="groupBody">
          <button class="cmd shapeTool" data-shape="freetext" title="${t("tip.freetext")}"><i data-lucide="type"></i><span>${t("lbl.textAdd")}</span></button>
          <button class="cmd" id="watermarkBtn" title="${t("tip.watermark")}"><i data-lucide="droplet"></i><span>${t("lbl.watermark")}</span></button>
          <button class="cmd" id="headerFooterBtn" title="${t("tip.headerFooter")}"><i data-lucide="heading"></i><span>${t("lbl.headerFooter")}</span></button>
        </div>
      </div>
      <div data-ribbon="edit" class="ribbonGroup">
        <div class="groupBody">
          <button class="cmd" id="editOcrBtn" title="${t("tip.ocr")}"><i data-lucide="scan-text"></i><span>${t("btn.ocr")}</span></button>
          <div class="menuHost">
            <button class="cmd menuTrigger" id="toOfficeBtn" aria-haspopup="true" aria-expanded="false" title="${t("lbl.toOffice")}"><i data-lucide="briefcase"></i><span>${t("lbl.toOffice")}</span></button>
            <div class="popover hidden" role="menu">
              <button class="mi" id="editExportDocxBtn" role="menuitem" title="${t("tip.expDocx")}"><i data-lucide="book-text"></i><span>Word</span></button>
              <button class="mi" id="editExportXlsxBtn" role="menuitem" title="${t("tip.expXlsx")}"><i data-lucide="table"></i><span>Excel</span></button>
            </div>
          </div>
        </div>
      </div>
      <div data-ribbon="forms" class="ribbonGroup">
        <div class="groupBody">
          <button class="cmd" id="formPrevBtn" title="${t("tip.formPrev")}"><i data-lucide="chevron-left"></i><span>${t("lbl.prev")}</span></button>
          <button class="cmd" id="formNextBtn" title="${t("tip.formNext")}"><i data-lucide="chevron-right"></i><span>${t("lbl.next")}</span></button>
        </div>
      </div>
      <div data-ribbon="forms" class="ribbonGroup">
        <div class="groupBody">
          <button class="cmd" id="formValidateBtn" title="${t("tip.formValidate")}"><i data-lucide="check"></i><span>${t("lbl.validate")}</span></button>
          <button class="cmd" id="formResetBtn" title="${t("tip.formReset")}"><i data-lucide="eraser"></i><span>${t("lbl.reset")}</span></button>
          <button class="cmd active" id="formHighlightBtn" title="${t("tip.formHighlight")}"><i data-lucide="eye"></i><span>${t("lbl.fieldHl")}</span></button>
        </div>
      </div>
      <span data-ribbon="forms" id="formHint" class="modeHint">${t("hint.noDoc")}</span>
      <div data-ribbon="sign" class="ribbonGroup">
        <div class="groupBody">
          <button class="cmd" id="signSignatureBtn" title="${t("tip.placeSignature")}"><i data-lucide="signature"></i><span>${t("btn.signature")}</span></button>
          <button class="cmd" id="signInitialsBtn" title="${t("tip.placeInitials")}"><i data-lucide="pen-tool"></i><span>${t("btn.initials")}</span></button>
          <button class="cmd menuTrigger" id="signManageBtn" title="${t("tip.manageSig")}"><i data-lucide="pen-line"></i><span>${t("lbl.manageSig")}</span></button>
        </div>
      </div>
      <div data-ribbon="sign" class="ribbonGroup">
        <div class="groupBody">
          <div class="menuHost">
            <button class="cmd menuTrigger" id="signMarksBtn" aria-haspopup="true" aria-expanded="false" title="${t("grp.fill")}"><i data-lucide="check"></i><span>${t("lbl.marks")}</span></button>
            <div class="popover hidden" role="menu">
              <button class="mi signMark" data-sign="check" role="menuitem" title="${t("tip.markCheck")}"><i data-lucide="check"></i><span>${t("tip.markCheck")}</span></button>
              <button class="mi signMark" data-sign="cross" role="menuitem" title="${t("tip.markCross")}"><i data-lucide="x"></i><span>${t("tip.markCross")}</span></button>
              <button class="mi signMark" data-sign="dot" role="menuitem" title="${t("tip.markDot")}"><i data-lucide="dot"></i><span>${t("tip.markDot")}</span></button>
              <button class="mi signMark" data-sign="date" role="menuitem" title="${t("tip.markDate")}"><i data-lucide="calendar"></i><span>${t("tip.markDate")}</span></button>
              <button class="mi signMark" data-sign="text" role="menuitem" title="${t("tip.signText")}"><i data-lucide="type"></i><span>${t("tip.signText")}</span></button>
            </div>
          </div>
          <label class="fieldCol" title="${t("tip.markColor")}">
            <input type="color" id="signColor" value="#1a3a8f" />
            <span class="fieldCaption"><i data-lucide="palette"></i>${t("lbl.color")}</span>
          </label>
        </div>
      </div>
      <div data-ribbon="sign" class="ribbonGroup">
        <div class="groupBody">
          <button class="cmd" id="dsignBtn" title="${t("tip.digitalSign")}"><i data-lucide="file-key-2"></i><span>${t("btn.digitalSign")}</span></button>
          <button class="cmd" id="dverifyBtn" title="${t("tip.verify")}"><i data-lucide="shield-check"></i><span>${t("btn.verify")}</span></button>
        </div>
      </div>
      <span data-ribbon="sign" id="signHint" class="modeHint">${t("hint.signPick")}</span>
      <span data-ribbon="organize" id="organizeHint" class="modeHint">${t("hint.organize")}</span>
      <div data-ribbon="convert" class="ribbonGroup">
        <div class="groupBody">
          <button class="cmd modeBtn active" data-conv="pdfa" title="${t("tip.convPdfa")}" type="button"><i data-lucide="file-check-2"></i><span>PDF/A</span></button>
        </div>
      </div>
      <div data-ribbon="convert" class="ribbonGroup">
        <div class="groupBody">
          <div class="menuHost">
            <button class="cmd menuTrigger" id="exportMenuBtn" aria-haspopup="true" aria-expanded="false" title="${t("grp.export")}"><i data-lucide="file-output"></i><span>${t("lbl.export")}</span></button>
            <div class="popover hidden" role="menu">
              <button class="mi" id="exportImagesBtn" role="menuitem" title="${t("tip.expImages")}"><i data-lucide="image"></i><span>PNG/JPG</span></button>
              <button class="mi" id="exportTextBtn" role="menuitem" title="${t("tip.expText")}"><i data-lucide="file-text"></i><span>${t("btn.expText")}</span></button>
              <button class="mi" id="exportHtmlBtn" role="menuitem" title="${t("tip.expHtml")}"><i data-lucide="file-code-2"></i><span>HTML</span></button>
              <button class="mi" id="exportDocxBtn" role="menuitem" title="${t("tip.expDocx")}"><i data-lucide="book-text"></i><span>Word</span></button>
              <button class="mi" id="exportXlsxBtn" role="menuitem" title="${t("tip.expXlsx")}"><i data-lucide="table"></i><span>Excel</span></button>
            </div>
          </div>
          <div class="menuHost">
            <button class="cmd menuTrigger" id="createMenuBtn" aria-haspopup="true" aria-expanded="false" title="${t("grp.import")}"><i data-lucide="file-plus"></i><span>${t("lbl.create")}</span></button>
            <div class="popover hidden" role="menu">
              <button class="mi" id="imagesToPdfBtn" role="menuitem" title="${t("tip.imagesToPdf")}"><i data-lucide="images"></i><span>${t("btn.imagesToPdf")}</span></button>
              <button class="mi" id="officeToPdfBtn" role="menuitem" title="${t("tip.officeToPdf")}"><i data-lucide="briefcase"></i><span>${t("btn.officeToPdf")}</span></button>
            </div>
          </div>
        </div>
      </div>
      <div data-ribbon="convert" class="ribbonGroup">
        <div class="groupBody">
          <button class="cmd" id="ocrBtn" title="${t("tip.ocr")}"><i data-lucide="scan-text"></i><span>${t("btn.ocr")}</span></button>
          <button class="cmd" id="ocrBatchBtn" title="${t("tip.ocrBatch")}"><i data-lucide="layers"></i><span>${t("btn.ocrBatch")}</span></button>
        </div>
      </div>
      <span data-ribbon="convert" class="modeHint">${t("hint.convert")}</span>
      <div data-ribbon="protect" class="ribbonGroup">
        <div class="groupBody">
          <button class="cmd" id="protectBtn" title="${t("tip.protect")}"><i data-lucide="lock"></i><span>${t("btn.protect")}</span></button>
          <button class="cmd" id="unprotectBtn" title="${t("tip.unprotect")}" disabled><i data-lucide="lock-open"></i><span>${t("btn.unprotect")}</span></button>
        </div>
      </div>
      <div data-ribbon="protect" class="ribbonGroup">
        <div class="groupBody">
          <button class="cmd shapeTool" data-shape="redact" title="${t("tip.redactTool")}"><i data-lucide="box-select"></i><span>${t("grp.redact")}</span></button>
          <button class="cmd" id="redactPatternBtn" title="${t("tip.redactPattern")}"><i data-lucide="regex"></i><span>${t("lbl.pattern")}</span></button>
          <button class="cmd" id="redactApplyBtn" disabled title="${t("tip.redactApply")}"><i data-lucide="check"></i><span>${t("lbl.applyRedact")}</span></button>
          <button class="cmd" id="redactDiscardBtn" disabled title="${t("tip.redactDiscard")}"><i data-lucide="x"></i><span>${t("lbl.discard")}</span></button>
        </div>
      </div>
      <div data-ribbon="protect" class="ribbonGroup">
        <div class="groupBody">
          <button class="cmd" id="sanitizeBtn" title="${t("tip.sanitize")}"><i data-lucide="eraser"></i><span>${t("grp.metadata")}</span></button>
          <button class="cmd" id="findHiddenBtn" title="${t("tip.findHidden")}"><i data-lucide="eye-off"></i><span>${t("grp.inspect")}</span></button>
        </div>
      </div>
      <span data-ribbon="protect" id="protectHint" class="modeHint"></span>
      <span data-ribbon="both" id="status" class="status"></span>
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
      <div id="emptyState"><img src="${logoText}" alt="Docura PDF" draggable="false" /></div>
      <div id="pageList" class="pageList"></div>
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
        <button id="mergeAddBtn" title="${t("tip.pickPdfs")}"><i data-lucide="plus"></i></button>
        <button id="mergeCommitBtn" disabled title="${t("tip.buildMerged")}"><i data-lucide="check"></i></button>
        <button id="mergeCancelBtn" title="${t("tip.cancel")}"><i data-lucide="x"></i></button>
        <span class="mergeHint">${t("hint.merge")}</span>
      </div>
      <div id="mergeList" class="mergeList"></div>
    </div>
    <div id="convertPane" class="mergePane convertPane hidden">
      <div class="mergeHeader">
        <strong>${t("convert.title")}</strong>
        <button id="convertAddBtn" title="${t("tip.pickPdfs")}"><i data-lucide="plus"></i></button>
        <select id="convertLevel" title="${t("tip.pdfaLevel")}">
          <option value="1b">PDF/A-1b</option>
          <option value="2b" selected>PDF/A-2b</option>
          <option value="3b">PDF/A-3b</option>
          <option value="4">PDF/A-4</option>
        </select>
        <button id="convertOutBtn" title="${t("tip.pickOutFolder")}"><i data-lucide="folder-open"></i></button>
        <button id="convertCommitBtn" disabled title="${t("tip.convertAll")}"><i data-lucide="check"></i></button>
        <button id="convertCancelBtn" title="${t("tip.cancel")}"><i data-lucide="x"></i></button>
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
    <div class="propsModal settingsModal">
      <div class="propsHeader"><strong>${t("set.title")}</strong><button id="settingsClose" class="propsClose">✕</button></div>
      <div class="settingsLayout">
        <nav class="settingsNav" id="settingsNav">
          <button class="settingsNavItem active" data-sec="secAppearance">${t("set.appearance")}</button>
          <button class="settingsNavItem" data-sec="secPerformance">${t("set.performance")}</button>
          <button class="settingsNavItem" data-sec="secPdfa">${t("set.pdfaExport")}</button>
          <button class="settingsNavItem" data-sec="secVprinter">${t("set.vprinter")}</button>
          <button class="settingsNavItem" data-sec="secSystem">${t("set.system")}</button>
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
            <div class="wmActions"><button id="pdfaSaveBtn">${t("btn.savePdfaNow")}</button><button id="gsRecheckBtn">${t("btn.recheckGs")}</button></div>
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
  <div id="pwOverlay" class="propsOverlay hidden">
    <div class="propsModal" style="width:24rem;">
      <div class="propsHeader"><strong>${t("pw.title")}</strong><button id="pwClose" class="propsClose">✕</button></div>
      <div class="propsPanel wmPanel">
        <span id="pwMessage" class="sigHint"></span>
        <label>${t("lbl.password")}<input id="pwInput" type="password" autocomplete="off" /></label>
        <div class="wmActions"><button id="pwOk">OK</button><button id="pwCancel">${t("tip.cancel")}</button></div>
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
          <button id="protectCommit">${t("btn.protectSave")}</button>
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
          <button id="patternCommit">${t("pattern.search")}</button>
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
          <button id="exportImgCommit">${t("btn.export")}</button>
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
          <button id="ocrCommit">${t("ocr.start")}</button>
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
  button.cmd { flex-direction:column; justify-content:center; gap:0.15rem;
               width:4.6rem; height:3.5rem; padding:0.2rem 0.1rem; line-height:1.08;
               border-radius:5px; box-sizing:border-box; }
  button.cmd svg.lucide { width:18px; height:18px; }
  button.cmd > span { font-size:0.6rem; color:var(--text-2); text-align:center; width:100%;
                      white-space:normal; overflow-wrap:break-word; hyphens:auto;
                      display:-webkit-box; -webkit-line-clamp:2; -webkit-box-orient:vertical;
                      overflow:hidden; }
  button.cmd:hover:not(:disabled) > span, button.cmd.active > span { color:inherit; }
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
  /* Lucide SVG icons: stroke follows the text color of the host button. */
  svg.lucide { width:16px; height:16px; stroke-width:2; flex:none; pointer-events:none; vertical-align:-3px; }
  .ribbonBody button, .commentHeader button, .mergeHeader button,
  .ribbonTab.settingsTab, .modeGroup .modeBtn {
    display:inline-flex; align-items:center; justify-content:center; gap:0.3rem;
  }
  .modeGroup .modeBtn svg.lucide, .signMark svg.lucide { width:15px; height:15px; }
  .commentItem .kind svg.lucide { width:13px; height:13px; }
  body.wheelZoomMode .viewer { cursor: grab; }
  body.wheelZoomMode.panning .viewer { cursor: grabbing; }
  body.wheelZoomMode .selectLayer, body.wheelZoomMode .drawLayer { pointer-events: none; }
  body.wheelZoomMode .pageSlot, body.wheelZoomMode .pageSlot * { user-select: none; -webkit-user-select: none; }
  /* A ribbon child is visible only when its data-ribbon matches the active tab
     or is "both" (the persistent quick-access cluster). One rule per tab. */
  .ribbonBody[data-active="reader"]   > [data-ribbon]:not([data-ribbon="reader"]):not([data-ribbon="both"]),
  .ribbonBody[data-active="comment"]  > [data-ribbon]:not([data-ribbon="comment"]):not([data-ribbon="both"]),
  .ribbonBody[data-active="view"]     > [data-ribbon]:not([data-ribbon="view"]):not([data-ribbon="both"]),
  .ribbonBody[data-active="edit"]     > [data-ribbon]:not([data-ribbon="edit"]):not([data-ribbon="both"]),
  .ribbonBody[data-active="forms"]    > [data-ribbon]:not([data-ribbon="forms"]):not([data-ribbon="both"]),
  .ribbonBody[data-active="sign"]     > [data-ribbon]:not([data-ribbon="sign"]):not([data-ribbon="both"]),
  .ribbonBody[data-active="convert"]  > [data-ribbon]:not([data-ribbon="convert"]):not([data-ribbon="both"]),
  .ribbonBody[data-active="organize"] > [data-ribbon]:not([data-ribbon="organize"]):not([data-ribbon="both"]),
  .ribbonBody[data-active="protect"]  > [data-ribbon]:not([data-ribbon="protect"]):not([data-ribbon="both"]) { display:none; }
  /* Convert is a full sub-view; it suppresses the quick-access cluster too. */
  .ribbonBody[data-active="convert"] > [data-ribbon="both"] { display:none; }
  .ribbonLabel { color:var(--text-3); font-size:0.85rem; margin-left:0.2rem; }
  /* Group = a divider-separated cluster of command buttons (no caption row;
     each button carries its own Foxit-style caption). */
  .ribbonGroup { display:inline-flex; align-items:stretch; align-self:stretch;
                 padding: 0 0.35rem; border-right:1px solid var(--border); }
  .ribbonGroup:last-of-type { border-right:none; }
  .ribbonGroup .groupBody { display:flex; align-items:center; gap:0.05rem; }
  .fieldCol { display:inline-flex; flex-direction:column; align-items:center; justify-content:center;
              gap:0.15rem; height:3.5rem; padding:0 0.25rem; box-sizing:border-box; }
  .fieldCol select { font-size:0.78rem; padding:0.1rem 0.25rem; }
  .fieldCol input[type=color] { height:1.5rem; width:2rem; padding:0; cursor:pointer; }
  .fieldCaption { font-size:0.62rem; color:var(--text-2); display:inline-flex; align-items:center;
                  gap:0.2rem; }
  .fieldCaption svg.lucide { width:12px; height:12px; }
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
  .pageList { display:flex; flex-direction:column; align-items:center; gap: 1rem; box-sizing:border-box; }
  .pageList.double { flex-flow: row wrap; justify-content:center; align-content:flex-start; gap: 0.6rem 0.6rem; }
  .pageList.single > .pageSlot:not(.current) { display:none; }
  .pageSlot { position:relative; background:white; box-shadow: 0 2px 10px rgba(0,0,0,0.35); flex: 0 0 auto; }
  .pageSlot img.page { display:block; width:100%; height:100%; }
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
  body.eraseMode .selectLayer { pointer-events:none; }
  body.eraseMode .pageSlot { cursor:cell; }
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
  #emptyState { position:absolute; inset:0; display:flex; align-items:center; justify-content:center;
                pointer-events:none; }
  #emptyState img { width:200px; opacity:0.85; filter:drop-shadow(0 4px 14px rgba(0,0,0,0.25)); }
  #emptyState.hidden { display:none; }
  .hiddenList { display:flex; flex-direction:column; gap:2px; font-size:0.85rem; color:var(--text-2); }
  .hiddenList div { white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
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
