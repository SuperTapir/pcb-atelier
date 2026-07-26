# Source references

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
