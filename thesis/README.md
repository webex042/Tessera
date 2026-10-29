# Tessera B.Tech Thesis — how to review and update

This folder holds the complete thesis for the Tessera project, written to the structure of
`Format for UG & PG Theses.pdf` (BIT Mesra), together with everything needed to regenerate it.

| File | What it is |
|---|---|
| `Tessera_Thesis.docx` | The editable thesis. This is the file to review, annotate and submit. |
| `Tessera_Thesis.pdf` | A PDF generated from exactly that DOCX, same pagination and layout. |
| `VERIFICATION_CHECKLIST.md` | **Read this first.** Everything that needs your or your guide's confirmation, plus every claim that is marked unverified. |
| `src/*.md` | The thesis text, one file per chapter, in a simple markdown-like format. |
| `src/tables.py` | All 21 tables as data. |
| `src/figures.py` | The figure registry: file, caption, print width, and the short title used in the List of Figures. |
| `figures/*.mmd` | Mermaid sources for the eight structural diagrams. |
| `figures/charts.py` | Matplotlib source for the four result charts. |
| `figures/*.png` | The rendered figures, at 300 dpi or better. |
| `build_thesis.py` | Builds the DOCX from `src/`. |
| `make_thesis.py` | Two-pass build: produces the DOCX **and** the PDF with the contents lists filled in. |

## Reviewing

Open `Tessera_Thesis.docx` in Word. Six yellow **[TO BE CONFIRMED]** boxes mark the places where a
detail was deliberately left blank rather than invented — the academic session and submission date,
the guide's designation, additional acknowledgements, the repository licence, and the Tessera /
Tessara naming question. `VERIFICATION_CHECKLIST.md` lists them with the reason for each.

The contents, List of Figures and List of Tables are real Word fields with correct page numbers
already cached in them, so they print correctly as they are. To refresh them after an edit, press
**Ctrl+A** then **F9** in Word, or right-click a list and choose *Update Field*. Word may also offer
to update them when the file opens.

The declaration page and the approval page are intentionally unsigned. Nothing in this folder should
be treated as approved until your guide has signed the approval page and you have read the whole
thesis and confirmed that the declaration is true.

## Making small edits

Edit `Tessera_Thesis.docx` in Word directly. That is the simplest route for wording changes, and it
is what you should do once your guide starts giving comments.

Note that editing the DOCX does **not** change `src/`. If you later re-run the build, your Word edits
are overwritten. Either stop using the build scripts once you start editing in Word, or make the
change in `src/` and rebuild.

## Rebuilding from source

Requires Python 3.11+ with `python-docx`, `pypdf`, `matplotlib` and `Pillow`; LibreOffice for the PDF;
and `@mermaid-js/mermaid-cli` for the diagrams.

```bash
# Regenerate the diagrams (only if you changed a .mmd file)
cd figures
mmdc -i fig_4_1.mmd -o fig_4_1.png -c ../mermaid-config.json -p ../puppeteer.json -b white -s 3
python3 charts.py          # regenerates the four result charts
cd ..

# Build the DOCX and the matching PDF, with contents lists populated
python3 make_thesis.py
```

`make_thesis.py` runs the build twice on purpose: the first pass lays the document out and converts
it to PDF so the real page numbers can be read back, and the second pass writes those numbers into
the contents fields. That is why the lists are correct in the PDF as well as in Word.

## The source format

`src/*.md` uses a small set of markers:

```
# CHAPTER 1|INTRODUCTION      chapter heading (starts a new page)
## 1.1|Section Title          section heading
### 1.1.1|Subsection Title    subsection heading
- item                        bullet
+ item                        numbered item
FIG:4.1                       place the figure registered under id 4.1
TBL:6.3                       place the table registered under id 6.3
!!text                        a [TO BE CONFIRMED] box
@REF|[1] ...                  a reference entry (references file only)
```

Inline, `**bold**`, `*italic*` and `` `code` `` all work. Triple-backtick blocks become monospaced
shaded blocks.

Figure and table numbers are manual, not automatic, so if you insert a new figure in the middle of a
chapter you must renumber the ones after it in `src/figures.py` and in the prose that refers to them.
The build will tell you if a figure or table is referenced but not defined.

## Formatting applied

A4; 1.5 inch left margin for binding and 1 inch elsewhere; Times New Roman 12 pt; 1.5 line spacing;
justified body text; lower-case roman numerals on the preliminary pages and arabic numerals
restarting at 1 for the body, both centred in the footer; IEEE-style numbered citations.

The supplied template places preliminary page numbers at the top of the page rather than the bottom.
This document puts them in the footer, which is the more common convention; if your department
requires the template's placement, it is a one-line change in `build_thesis.py`
(`footer_page_number`).
