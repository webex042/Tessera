#!/usr/bin/env python3
"""Two-pass build of the thesis: DOCX with populated contents lists, plus a matching PDF.

Pass 1 lays the document out with empty contents fields and converts it to PDF so that
real page numbers can be read back. Pass 2 rebuilds the DOCX with those page numbers
cached inside genuine Word TOC fields, so the lists are correct in the PDF and still
update automatically in Word. The PDF is then regenerated from the final DOCX.
"""
import re
import subprocess
import sys
import pathlib
import tempfile

import pypdf

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import build_thesis  # noqa: E402
sys.path.insert(0, str(HERE / "src"))
from figures import SHORT as FIG_SHORT  # noqa: E402
from tables import SHORT as TBL_SHORT   # noqa: E402

OUT_DOCX = HERE / "Tessera_Thesis.docx"
OUT_PDF = HERE / "Tessera_Thesis.pdf"


def soffice_pdf(docx, outdir):
    subprocess.run(
        ["soffice", "--headless", "--convert-to", "pdf", "--outdir", str(outdir), str(docx)],
        check=True, capture_output=True, timeout=600,
    )
    return pathlib.Path(outdir) / (pathlib.Path(docx).stem + ".pdf")


def norm(s):
    s = s.replace("—", "-").replace("–", "-").replace("’", "'")
    s = s.replace("“", '"').replace("”", '"').replace("≤", "<=")
    s = s.replace("≥", ">=").replace("×", "x").replace(" ", " ")
    return re.sub(r"\s+", " ", s).strip().lower()


def page_labels(reader):
    """Displayed page label per physical page, read from the footer line."""
    labels = []
    for pg in reader.pages:
        txt = (pg.extract_text() or "").strip().split("\n")
        lab = ""
        for line in reversed(txt):
            t = line.strip()
            if t:
                lab = t
                break
        labels.append(lab if re.fullmatch(r"[ivxlcdm]+|\d+", lab) else "")
    return labels


def locate(reader, labels, targets, start_page=0):
    """Map each target string to the displayed label of the page it first appears on."""
    pages = [norm(p.extract_text() or "") for p in reader.pages]
    found = {}
    cursor = start_page
    for key, needle in targets:
        n = norm(needle)
        # headings can wrap, so try progressively shorter prefixes
        probes = [n]
        if len(n) > 60:
            probes.append(n[:60])
        if len(n) > 40:
            probes.append(n[:40])
        hit = None
        for probe in probes:
            for idx in range(cursor, len(pages)):
                if probe and probe in pages[idx]:
                    hit = idx
                    break
            if hit is not None:
                break
        if hit is None:
            for probe in probes:
                for idx in range(0, len(pages)):
                    if probe and probe in pages[idx]:
                        hit = idx
                        break
                if hit is not None:
                    break
        if hit is not None:
            found[key] = labels[hit] or str(hit + 1)
            cursor = hit
        else:
            found[key] = "?"
    return found


def main():
    with tempfile.TemporaryDirectory() as td:
        td = pathlib.Path(td)
        p1 = td / "pass1.docx"
        figs, tbls, outline = build_thesis.main(str(p1))
        pdf1 = soffice_pdf(p1, td)
        reader = pypdf.PdfReader(str(pdf1))
        labels = page_labels(reader)

        # the body starts at the first page whose label is arabic "1"
        body_start = next((i for i, l in enumerate(labels) if l == "1"), 0)

        head_targets = [((lvl, txt), txt) for lvl, txt in outline]
        heads = locate(reader, labels, head_targets, body_start)

        fig_targets = [(("fig", fid), f"Figure {fid}:") for fid, _ in figs]
        figpages = locate(reader, labels, fig_targets, body_start)

        tbl_targets = [(("tbl", tid), f"Table {tid}:") for tid, _ in tbls]
        tblpages = locate(reader, labels, tbl_targets, body_start)

    def trim(text, limit=84):
        """Use the caption's first sentence in the lists, so entries stay on one line."""
        first = text.split(". ")[0].rstrip(".")
        if 15 <= len(first) <= limit:
            return first
        if len(text) <= limit:
            return text.rstrip(".")
        return text[:limit].rsplit(" ", 1)[0].rstrip(" ,;") + "…"

    cache = {
        "toc": [(lvl, txt, heads[(lvl, txt)]) for lvl, txt in outline],
        "lof": [(1, f"Figure {fid}:  {FIG_SHORT.get(fid) or trim(cap)}",
                 figpages[("fig", fid)]) for fid, cap in figs],
        "lot": [(1, f"Table {tid}:  {TBL_SHORT.get(tid) or trim(cap)}",
                 tblpages[("tbl", tid)]) for tid, cap in tbls],
    }

    missing = [k for k, v in {**heads, **figpages, **tblpages}.items() if v == "?"]
    if missing:
        print(f"WARNING: could not locate {len(missing)} entries: {missing[:6]}")

    build_thesis.main(str(OUT_DOCX), cache=cache)
    final = soffice_pdf(OUT_DOCX, HERE)
    reader2 = pypdf.PdfReader(str(final))
    print(f"DOCX: {OUT_DOCX}")
    print(f"PDF : {final}  ({len(reader2.pages)} pages)")
    print(f"contents entries: {len(cache['toc'])}  figures: {len(cache['lof'])}  tables: {len(cache['lot'])}")


if __name__ == "__main__":
    main()
