# FeatherPDF

[![Discord](https://img.shields.io/badge/Discord-Maragon.Dev-5865F2?logo=discord&logoColor=white)](https://discord.gg/b4Epb4eSSH)

**The PDF editor of the FeatherWorks suite · version 0.1.9**

The interface is in Portuguese for now; the menu and button names below are translated.

## Download

[![Get it from the Snap Store](https://snapcraft.io/static/images/badges/en/snap-store-black.svg)](https://snapcraft.io/featherpdf)

| System | File | How to install |
|---|---|---|
| **Windows 10/11** | [FeatherPDF-windows-x64.exe](https://github.com/feather-works/featherpdf/releases/latest/download/FeatherPDF-windows-x64.exe) | run the installer; or use the [portable zip](https://github.com/feather-works/featherpdf/releases/latest/download/FeatherPDF-windows-x64-portatil.zip) |
| **Linux** (Snap Store) | [snapcraft.io/featherpdf](https://snapcraft.io/featherpdf) | `sudo snap install featherpdf` (updates by itself) |
| **Linux** (Ubuntu, Debian, Mint…) | [featherpdf-linux-amd64.deb](https://github.com/feather-works/featherpdf/releases/latest/download/featherpdf-linux-amd64.deb) | `sudo apt install ./featherpdf-linux-amd64.deb` |
| **Linux** (any distribution) | [featherpdf-linux-x86_64.tar.gz](https://github.com/feather-works/featherpdf/releases/latest/download/featherpdf-linux-x86_64.tar.gz) | extract it and run `./instalar.sh` |

Every version and the `SHA256SUMS` are on the [releases page](https://github.com/feather-works/featherpdf/releases).

### In the next version

- **Text becomes PDF:** open or drop a `.txt` and it becomes an A4 PDF with the running text (also when merging files and in workflows); the converted image or text already comes with the file name to save.
- **Comment summary as PDF:** in the Comments panel, *Summary PDF…* makes a PDF with every comment (page, author, date, status and replies) to print or send.
- **Safer redaction:** "Also remove metadata" now also removes the XMP of pages and images and the EXIF of photos (camera, GPS), like Clean and sanitize; before, only the document's own metadata went.
- **Compress removes broken links and bookmarks:** with cleanup on, a link or bookmark that points to a page that no longer exists (left over from pages deleted in another program) is removed; links to websites and bookmarks that only group others stay.
- **Sanitize removes embedded video, sound and 3D** together with attachments (the media file travels inside the PDF).
- **Auto-scroll** (View, or Ctrl+Shift+H): the document scrolls down by itself so you can read without the mouse; ↓ speeds up, ↑ slows down, Esc stops.
- **No frozen window on large PDFs:** search (Ctrl+F), redact, password-protect, attach a file, copy and cut pages and send to FeatherDocs run in the background; if you edit the document meanwhile, nothing you did is lost.
- **PDF to Word:** punctuation right after a link or another text piece no longer gets a space before it.

### New in 0.1.9

- **Rotate the view:** Ctrl+Shift+= and Ctrl+Shift+- (or the rotate button in the footer) turn the page on screen in 90° steps, to read a sideways plan or table; the file does not change. Fields, comments, search, measurements and selection follow the rotation.
- **Dates in form fields like in Acrobat:** a field with a date format (`AFDate_FormatEx`) accepts "5/3/85", "05032026" or "5 março 2026" and stores it in the field's format ("05/03/1985", "2026-03-05", "mar 5, 2026"…); a date that does not exist (31/02) is refused with the right format in the warning, and the calendar reads and writes the same format.
- **Less memory after closing:** closing a PDF gives back to the system the images kept for faster drawing (up to 256 MB) and everything that belonged only to that document; on a 134-page manual with photos, the app goes from 232 MB back to 28 MB.

### New in 0.1.8

- **New start screen:** "Continue where you left off" with the page you were on, tools in tiles and recent files as cards with the cover of page 1.
- **Nicer dark theme:** warm graphite instead of near-black gray, thinner icons and a readable red; a dark page also darkens logos on white and scanned pages.
- **Confirmation on save:** exporting ends on a "Done!" screen with Open file and Open folder; split, extract, compress, attachments and the other tools show a "Saved" card with the same buttons.
- **Presentation mode (F5):** one page at a time in full screen; arrows, space or Page Down move on, Esc returns as it was. Ctrl+J merges files.
- **Safer, without losing files:** Ctrl+Z after Save as, the same PDF in two tabs, removing the password and saving over a signed original no longer lose anything; redaction erases a letter that is half covered, fields, comments and XFA; field scripts run in a separate process, with a time limit.
- **Faster on large PDFs:** OCR off the window thread and without duplicated text, less memory with many pages, merging keeps bookmarks and links.

### New in 0.1.7

- **Opens on computers with old graphics cards:** on cards without usable Vulkan/OpenGL (Intel HD 4000, for example) FeatherPDF closed at start; now the window opens drawn by the CPU. If the card fails while the app is open, the windows move to the CPU instead of the app closing.
- **No internet:** FeatherPDF no longer touches the network; the new-version notice and the OCR model download are gone (the built-in Latin OCR stays).
- **Pages:** copy, cut and paste pages (also between open PDFs) and logical page numbers (i, ii… in the preface, A-1…).
- **Bookmarks and links:** create, rename, move and delete bookmarks; a page Links panel to change or remove the target of each link.
- **Forms:** required, read-only and default value for fields.
- **Attachments, images and zoom:** attach a file to the PDF (and remove one); rotate an image 90°; zoom to an area (drag a rectangle and it fills the screen).
- **Safer with odd PDFs:** a malicious PDF no longer crashes the app (compression bomb, huge field formula); redaction no longer lands on the wrong page after editing and also removes the alternative text of tags; compress and split keep the password.
- **More:** paper size when printing (A4, Letter, Legal, A3, A5), extract the photos of a PDF, comment opacity, editable and removable header and footer, search also in fields and attachments.

### New in 0.1.6

- **Document properties (Ctrl+D):** editable title, author, subject and keywords (accents work, Ctrl+Z undoes), plus file, size, pages, PDF version, dates and the producing program, as in Acrobat.
- **TIFF becomes PDF:** open, drop, Images to PDF and Merge accept `.tif`/`.tiff` from the scanner; each TIFF page becomes a page (black and white/fax, LZW, JPEG, gray, color, CMYK).
- **Edit a comment:** Comments ▸ Edit changes the text of an existing note, markup or text box; the text box is redrawn on the page.
- **Default PDF reader:** Settings ▸ Make default reader (on Linux through the system; on Windows it shows up in Default apps).
- **Open large PDFs with less memory:** the file is read straight from disk, without a whole copy in memory (#2).
- **More:** Split and Extract pages on the start screen; bank slips with junk before `%PDF-` open as in Acrobat; recent files with odd names come back whole.

### New in 0.1.5

- **Measurements match the plan:** the scale CAD programs write into the PDF is now read also when it has no unit, and the distance factor is applied correctly. On a lot plan, the measured sides now match the dimensions.
- **Move the points of a measurement:** with a Measure tool on, drag a point of a finished measurement to adjust it; the value updates at once.
- **Measure on large plans without freezing:** the snap reads the drawing's corners once and only searches near the mouse (a plan with 375 thousand vertices became light to zoom).
- **Save without freezing:** saving and preparing the PDF drawing run in the background; editing during the save is not lost.
- **Zoom:** you can type the zoom, there is a "Height" button, and zooming at the mouse no longer jumps when a small page starts to fill the screen.
- **More:** export a page range, BMP and GIF; paste an image or text from the clipboard; Sanitize can remove comments; "Clean notebook page" leaves photos and logos alone.

## How it works

A native editor in **Rust + pelicano**. The page is shown as the real PDF (text, images and graphics), and the
document stays vector for editing and saving. Display uses **Hayro 0.8**, a rasterizer written in Rust, so neither
PDFium nor MuPDF has to be installed.

| Crate | Job |
|---|---|
| `featherpdf-core` | PDF objects through `lopdf`: text extraction, rotation, deletion, limited text replacement, forms, redaction, serialization. **No IO or interface dependency.** |
| `featherpdf-io` | between core, the suite libraries and rendering: create, open, save, render a page, convert to and from other formats. |
| `featherpdf-ocr` | local OCR with a built-in Latin model; nothing is downloaded. |
| `featherpdf-pelicano` | window, navigation, zoom and editing commands; no PDF parser or codec of its own. |
| `feather-io` | bounded reads and atomic writes shared by the suite; macros sandbox (`feather-macros`). |
| `feather-draw`, `feather-shapes` | the vector A4 scene used to create documents, and shapes. |
| `feather-ui` | theme, title bar, ribbon, F1 help, shortcuts and windows. |
| `hayro` | turns existing PDFs into pixels for the screen (it never changes the file). |

**Important:** the rasterized preview is only what the window shows. The saved PDF is **not** a screenshot; its
vectors and objects stay in the document.

## Features

- Open PDF files up to **128 MiB**, read from disk.
- Zoom from **50% to 300%**, page navigation, side panel, text view.
- Rotate pages in 90° steps and delete pages (keeping at least one); copy, cut and paste pages.
- Replace text when the stream and the font allow it; insert text, images, drawings and equations; comments and markup.
- Forms: create, fill, calculated fields, import and export data.
- Local OCR. FeatherPDF never touches the internet.
- Redaction that removes the content from the file; cleanup of JavaScript, attachments and metadata; passwords
  (AES-256/AES-128) and PAdES digital signatures with an A1 certificate.
- JavaScript in the suite's macro sandbox (no files, no network).
- Convert: FeatherSheet/Excel, Word/ODT, PowerPoint, PDF/A, text, Markdown, HTML and images.
- Print through CUPS (`lp`).
- **Atomic** saving: written to a temporary file and renamed only when the operation ends without error; symbolic
  links and the existing file's permissions are kept.
- Heavy rendering off the window thread; stale results are dropped when the page, document or zoom change.

## Commands

```bash
cargo run -p featherpdf-pelicano
cargo check --workspace --all-targets
cargo test --workspace
```

pelicano comes from crates.io and the suite libraries from git tags; the `[patch]` in `Cargo.toml` only swaps hayro
for the copy in `vendor/`.

## Limits

This is **not yet an editor with 100% of the features**. Text replacement is partial. Rare fonts and complex graphic
effects may not render correctly in Hayro. Editing a signed PDF and saving rewrites the whole file, so the old
signatures stop being valid (Ctrl+S refuses to save over a signed original). The verifier checks integrity, not
identity: the chain up to a trusted certificate authority is not validated.

Rendering is limited to **20 million pixels per page**, so no unbounded buffers are created. The original file is
never rasterized or covered automatically.

**Validation:** `cargo test --workspace` covers core, io, OCR and the interface logic; the interface itself is checked by hand.

License: GPL-3.0.
