// Shared drawing helpers for the Tessera deck.
// The repeating visual motif is a thin outlined circle (ensō): it marks steps,
// statuses and the title slide. No accent stripes, no rules under titles.

const { HEX } = require("./theme");

const F = {
  head: "Cambria",
  body: "Calibri",
  mono: "Courier New",
};

/** Thin outlined circle — the deck's motif. */
function enso(slide, { x, y, d, color = HEX.shu, w = 1, name }) {
  slide.addShape("ellipse", {
    x, y, w: d, h: d,
    fill: { type: "none" },
    line: { color, width: w },
    objectName: name,
  });
}

/** Numbered step marker: outlined circle with a figure inside. */
function stepMark(slide, { x, y, n, d = 0.42, color = HEX.shu, textColor }) {
  slide.addShape("ellipse", {
    x, y, w: d, h: d,
    fill: { type: "none" },
    line: { color, width: 1 },
    objectName: `step-${n}-ring`,
  });
  slide.addText(String(n), {
    x, y, w: d, h: d,
    align: "center", valign: "middle",
    fontFace: F.body, fontSize: 11, bold: true,
    color: textColor || color,
    margin: 0, isTextBox: true,
    objectName: `step-${n}-num`,
  });
}

/** Small filled status dot. */
function dot(slide, { x, y, d = 0.13, color, name }) {
  slide.addShape("ellipse", {
    x, y, w: d, h: d,
    fill: { color },
    line: { color, width: 0 },
    objectName: name,
  });
}

/** A content card: subtle tint, generous padding, no edge stripe. */
function card(slide, { x, y, w, h, fill = HEX.mist, line, name, radius = 0.04 }) {
  slide.addShape("roundRect", {
    x, y, w, h,
    rectRadius: radius,
    fill: { color: fill },
    line: line ? { color: line, width: 0.75 } : { type: "none" },
    objectName: name,
  });
}

/** Heading + body inside a card (or anywhere). */
function cardText(slide, { x, y, w, title, body, titleSize = 13, bodySize = 10.5,
                           titleColor = HEX.ink, bodyColor = HEX.stone, gap = 0.26, name }) {
  slide.addText(title, {
    x, y, w, h: gap,
    fontFace: F.body, fontSize: titleSize, bold: true, color: titleColor,
    align: "left", valign: "top", margin: 0, isTextBox: true,
    objectName: name ? `${name}-t` : undefined,
  });
  if (body) {
    slide.addText(body, {
      x, y: y + gap, w, h: 1.4,
      fontFace: F.body, fontSize: bodySize, color: bodyColor,
      align: "left", valign: "top", margin: 0, lineSpacingMultiple: 1.12,
      isTextBox: true,
      objectName: name ? `${name}-b` : undefined,
    });
  }
}

/** Thin horizontal connector with an arrowhead. */
function arrow(slide, { x, y, w, color = HEX.dryStone, width = 1, name }) {
  slide.addShape("line", {
    x, y, w, h: 0,
    line: { color, width, endArrowType: "triangle" },
    objectName: name,
  });
}

/** Thin vertical connector with an arrowhead. */
function arrowDown(slide, { x, y, h, color = HEX.dryStone, width = 1, name }) {
  slide.addShape("line", {
    x, y, w: 0, h,
    line: { color, width, endArrowType: "triangle" },
    objectName: name,
  });
}

/** Plain thin rule used only inside diagrams, never under a title. */
function hairline(slide, { x, y, w, color = HEX.dryStone, width = 0.75, dash, name }) {
  const line = { color, width };
  if (dash) line.dashType = dash;
  slide.addShape("line", { x, y, w, h: 0, line, objectName: name });
}

/** A labelled box used in the architecture and flow diagrams. */
function box(slide, { x, y, w, h, label, sub, fill = HEX.white, border = HEX.dryStone,
                      labelColor = HEX.ink, subColor = HEX.stone, size = 10.5, subSize = 8.5,
                      bold = true, name }) {
  slide.addShape("roundRect", {
    x, y, w, h,
    rectRadius: 0.05,
    fill: { color: fill },
    line: { color: border, width: 0.75 },
    objectName: name,
  });
  const runs = [{ text: label, options: { bold, fontSize: size, color: labelColor, breakLine: !!sub } }];
  if (sub) runs.push({ text: sub, options: { bold: false, fontSize: subSize, color: subColor } });
  slide.addText(runs, {
    x: x + 0.06, y, w: w - 0.12, h,
    fontFace: F.body, align: "center", valign: "middle",
    margin: 0, lineSpacingMultiple: 1.05, isTextBox: true,
    objectName: name ? `${name}-label` : undefined,
  });
}

/** Dashed container marking a process or trust boundary. */
function boundary(slide, { x, y, w, h, label, color = HEX.stoneLight, name }) {
  slide.addShape("roundRect", {
    x, y, w, h,
    rectRadius: 0.03,
    fill: { type: "none" },
    line: { color, width: 0.75, dashType: "dash" },
    objectName: name,
  });
  if (label) {
    slide.addText(label, {
      x: x + 0.1, y: y - 0.02, w: w - 0.2, h: 0.24,
      fontFace: F.body, fontSize: 8.5, bold: true, color,
      charSpacing: 1, align: "left", valign: "middle",
      margin: 0, isTextBox: true,
      objectName: name ? `${name}-cap` : undefined,
    });
  }
}

/** Caption line, used under diagrams. */
function caption(slide, { x, y, w, text, color = HEX.stone, size = 9.5, align = "left", name }) {
  slide.addText(text, {
    x, y, w, h: 0.34,
    fontFace: F.body, fontSize: size, italic: true, color,
    align, valign: "top", margin: 0, isTextBox: true,
    objectName: name,
  });
}

module.exports = { F, enso, stepMark, dot, card, cardText, arrow, arrowDown, hairline, box, boundary, caption };
