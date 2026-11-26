# Third-party notices: 3D support

The 3D feature (U3D decoder in `crates/pdf_3d`, 3D view and model import in
the desktop UI) uses the third-party work listed here. The license texts in
this folder ship with the installer under `licenses/3d/`.

## Universal 3D Sample Software (Apache License 2.0)

Copyright Intel Corporation and contributors. Source:
<https://github.com/ningfei/u3d> (mirror of the U3D sample software from
SourceForge). License text: [`Apache-2.0.txt`](Apache-2.0.txt).

- `crates/pdf_3d/src/u3d/clod.rs` is a derivative work: a port to Rust of
  the progressive mesh decoder `CIFXAuthorCLODDecoder_P.cpp` (with the
  contexts of `IFXACContext.h`). Changes against the original: rewritten in
  Rust, reconstructs only the final resolution (face updates are applied
  directly instead of being kept for later resolution changes), bounds
  checks for untrusted input.
- `crates/pdf_3d/tests/fixtures/intel/*.u3d` are unmodified test scenes from
  `Samples/TestScenes/`, used only as test input (not part of the
  installer).

The bit stream reader (`crates/pdf_3d/src/u3d/bits.rs`) follows the
reference code in Annex A of ECMA-363 (Universal 3D File Format, 4th
edition), <https://ecma-international.org/publications-and-standards/standards/ecma-363/>.

## three.js (MIT)

Copyright © 2010-2026 three.js authors. Version 0.187.0,
<https://github.com/mrdoob/three.js>. License text:
[`three.js-LICENSE.txt`](three.js-LICENSE.txt).

Bundled into the desktop UI: the core library and, from `examples/jsm`,
`OrbitControls`, the glTF, OBJ, MTL, STL, PLY and 3MF loaders and
`BufferGeometryUtils`.

## fflate (MIT)

Copyright (c) 2023 Arjun Barrett. Version 0.8.3 as distributed with three.js
(`examples/jsm/libs/fflate.module.js`), <https://github.com/101arrowz/fflate>.
Used by the 3MF loader to unpack the archive. License text:
[`fflate-LICENSE.txt`](fflate-LICENSE.txt).
