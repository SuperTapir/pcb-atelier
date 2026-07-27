# Source references

发布验收与性能记录见
[`release-validation-2026-07-27.md`](release-validation-2026-07-27.md)。

This directory is a local study shelf, not a dependency directory. Third-party
clones are intentionally ignored by Git; only this index is versioned. We may
study their interaction, rendering, state-management, and export approaches,
but must not copy code or make the product architecture depend on them without
an explicit licensing and design decision.

## Local predecessor

- `/Users/tapir/Development/PCB_lightgraph_mac` — the validated downstream
  reference for project packaging, artwork-to-production conversion, Gerber /
  Excellon export, EasyEDA handoff, and native `.eprj2` conversion. Its old
  one-image/fixed-recipe document model is deliberately not inherited.

## Cloned source studies

| Directory | Project | Revision | License | Study focus |
| --- | --- | --- | --- | --- |
| `fabric.js` | [Fabric.js](https://github.com/fabricjs/fabric.js) | `e009409980c199ee2c1bcbc42ef1a3689105f1db` | MIT | Object selection, transforms, groups, hit-testing, canvas viewport and serialization. |
| `filerobot-image-editor` | [Filerobot Image Editor](https://github.com/scaleflex/filerobot-image-editor) | `a7cce3173cb380c4900b9cb9fb8d91f7f33a7a26` | MIT | Image-editor workspace layout, crop/transform controls, history and persisted design state. |
| `svgedit` | [SVG-Edit](https://github.com/SVG-Edit/svgedit) | `46736931e886ccbf14d6beda00c73a21ff1d3fc6` | MIT | Precise SVG editing, layer semantics, selection UI and import/export separation. |

## JLCPCB manufacturing capability snapshot

The `jlcpcb-fr4-art-v2026.04` profile uses only first-party JLCPCB material.
The pages below were rechecked on 2026-07-27. They are public factual
references, not source-code dependencies: JLCPCB retains copyright in the page
text and images, no reusable software/content license is stated, and this
project records paraphrased manufacturing facts and links only. No page text,
images, or production assets are redistributed.

| Official source | Source version/date | Study purpose | Facts used by PCB Atelier |
| --- | --- | --- | --- |
| [PCB Manufacturing & Assembly Capabilities](https://jlcpcb.com/capabilities/Capabilities) | No page revision date shown; accessed 2026-07-27 | Establish the common FR-4 capability envelope. | FR-4 supports 1–32 copper layers; the listed common thicknesses are 0.4/0.6/0.8/1.0/1.2/1.6/2.0 mm; finished outer copper is 1/2 oz for multilayer, with heavier options for 2-layer boards; solder mask colors are green, purple, red, yellow, blue, white, and black. The table describes HASL (leaded/lead-free) and ENIG for FR-4, while OSP is identified as copper-core-only. |
| [JLCPCB Copper Weight (Thickness) Guide](https://jlcpcb.com/help/article/jlcpcb-copper-weight) | Updated 2025-12-15; accessed 2026-07-27 | Distinguish outer- from inner-layer copper choices. | Standard FR-4 outer copper is 1 oz or 2 oz. The guide lists 0.5 oz as an inner-layer default/option, so `Oz0_5` remains a stable domain value but is not offered by this FR-4 outer-copper snapshot. |
| [JLCPCB Surface Finish](https://jlcpcb.com/help/article/jlcpcb-surface-finish) | Updated 2025-04-24; accessed 2026-07-27 | Verify exposed-copper finish semantics. | The dedicated guide lists leaded HASL, lead-free HASL, and ENIG. Together with the capability table, this excludes OSP from the current FR-4 profile while retaining `Osp` as a domain value for other future profiles. |
| [How to Design Multi-color Silkscreen using EasyEDA](https://jlcpcb.com/help/article/how-to-design-multi-color-silkscreen-using-easyeda) | Updated 2026-04-14; accessed 2026-07-27 | Capture color-silkscreen ordering constraints and handoff limitations. | Multi-color silkscreen requires EasyEDA Pro color-silkscreen production data and is limited to 2/4 layers, white solder mask, 1 oz outer copper, ENIG with 1 μin gold, and the documented delivery/via-covering choices. Because its color data is separate from ordinary Gerber and the current adapter cannot emit it losslessly, PCB Atelier must not describe an ordinary export as directly orderable multi-color output. |

The earlier OpenSpec draft listed OSP as a regular JLCPCB FR-4 finish. That
conflicts with the first-party capability table and dedicated finish guide
above. The implementation follows the official evidence: the stable
`SurfaceFinish::Osp` semantic remains available for future manufacturer or
substrate profiles, but `jlcpcb-fr4-art-v2026.04` rejects it.

## Professional app start-screen study

These first-party product documents were checked on 2026-07-27. They are
interaction references only: no page text, screenshots, templates, or product
assets are redistributed, and no third-party runtime dependency was added.
Official web pages do not expose a source commit or reusable software license,
so the dated documentation version is recorded instead.

| Official source | Version / license note | Study purpose | Decision for PCB Atelier |
| --- | --- | --- | --- |
| [Adobe Photoshop Home screen overview](https://helpx.adobe.com/photoshop/desktop/get-started/learn-the-basics/homescreen-overview.html) | Updated 2026-02-23; Adobe page copyright, no reusable software license stated | Understand how an established creative tool prioritizes entry actions and real file data. | Keep **New** and **Open** as the dominant actions; use document presets to reduce setup cost; do not show a Recents section until PCB Atelier owns a truthful recent-file index. |
| [Sketch: Creating, opening & viewing documents](https://www.sketch.com/docs/getting-started/creating-opening-and-viewing-documents/) and [Templates](https://www.sketch.com/docs/symbols-and-styles/templates/) | Updated 2026-06-22 and 2026-04-07; Sketch page copyright, no reusable software license stated | Study the Workspace window, local-file opening, document browsing, and template-based creation. | Treat the start screen as a working project hub, but only expose capabilities backed by the desktop app: local `.pcba` opening, common-size starters, custom dimensions, and existing local settings. |
| [KiCad 9.0: Getting Started](https://docs.kicad.org/9.0/en/getting_started_in_kicad/getting_started_in_kicad.html) | KiCad 9.0 documentation; documentation contributor copyright, no content copied | Confirm the EDA convention of beginning from a named project and keeping project-level operations separate from editing tools. | Keep project lifecycle at the start screen and editor project menu. Creating a size preset still produces a normal PCB Atelier project rather than a second template schema. |

The deliberately local implementation is smaller than a Photoshop or Sketch
home system: it adds useful creation presets and custom board dimensions while
avoiding cloud libraries, tutorials, and fake recent files. Those features
would require their own data model and acceptance cases before appearing in the
interface.

## EasyEDA image-vectorization study

The first-party EasyEDA Pro documentation below was checked on 2026-07-27.
It is used only to understand the target format and interaction semantics; no
EasyEDA source code or assets are redistributed, and no runtime dependency was
added.

| Official source | Version / license note | Study purpose | Decision for PCB Atelier |
| --- | --- | --- | --- |
| [Place Image](https://prodocs.easyeda.com/en/pcb/place-image/index.html) | Current online EasyEDA Pro guide; page copyright, no reusable software license stated | Confirm that native image placement exposes tolerance, simplification, smoothing and despeckling rather than scaling a production raster. | Treat production pixels as samples, not final visible polygon corners; provide deterministic contour simplification before handoff. |
| [`PCB_MathPolygon.convertImageToComplexPolygon`](https://prodocs.easyeda.com/en/api/reference/pro-api.pcb_mathpolygon.convertimagetocomplexpolygon.html) | Beta Extension API, accessed 2026-07-27 | Confirm the native image-import result and parameter semantics. | Keep PCB Atelier's Rust pipeline as the source of truth; do not depend on the Beta runtime API, but match its complex-polygon quality goal. |
| [`TPCB_PolygonSourceArray`](https://prodocs.easyeda.com/en/api/reference/pro-api.tpcb_polygonsourcearray.html) | Current Extension API type reference, accessed 2026-07-27 | Verify supported polygon commands and units. | EasyEDA paths support linear, arc and cubic Bézier segments. The first compatibility step retains static `FILL + L` and removes raster stair steps within a one-production-pixel physical error bound. |

## How these references guide PCB Atelier

The product document remains the source of truth. We will build our own model:

```text
card side (front / back)
  → content layers and groups
  → mapping to production layers
  → shared fabrication geometry
  → 2D production preview, 3D material preview, EDA / Gerber export
```

An editor library may render and manipulate content objects, but it must not
become the project model or production-layer truth source.
