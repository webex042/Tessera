// Tessera deck — Zen visual system.
// Warm off-white washi, sumi-ink charcoal, stone grey, one muted vermilion accent.
// Sage is reserved for "verified" markers and slate for neutral diagram fills; both
// are muted enough to stay inside the restrained palette the brief asks for.

const THEME = {
  name: "Tessera Zen",
  headFontFace: "Cambria",
  bodyFontFace: "Calibri",
  colors: {
    dk1: "201E1B", // sumi ink  — primary text on light, background on dark slides
    lt1: "F6F4F0", // washi     — warm off-white page
    dk2: "6E675E", // stone     — secondary text, captions
    lt2: "E7E3DA", // mist      — card tints, rules
    accent1: "A85A45", // shu (vermilion) — the single accent
    accent2: "6C7F6B", // sage  — verified / met
    accent3: "4A5A66", // slate — neutral diagram fill
    accent4: "C4BCAE", // dry stone — muted dividers
    accent5: "8C8577", // stone light
    accent6: "3A3733", // ink light
    hlink: "A85A45",
    folHlink: "8C6355",
  },
};

// Raw hex for the options that refuse scheme colours.
const HEX = {
  ink: "201E1B",
  washi: "F6F4F0",
  stone: "6E675E",
  mist: "E7E3DA",
  shu: "A85A45",
  sage: "6C7F6B",
  slate: "4A5A66",
  dryStone: "C4BCAE",
  stoneLight: "8C8577",
  inkLight: "3A3733",
  white: "FFFFFF",
};

module.exports = { THEME, HEX };
