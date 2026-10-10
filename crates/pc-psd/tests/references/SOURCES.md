# Photoshop slices

`photoshop-cs6-slices-v7.dat` is the version-16 descriptor tail extracted from
[ag-psd's Photoshop CS6 slice fixture](https://github.com/Agamnentzar/ag-psd/blob/04d927df87130ad6d765bc63d51ade0a98d66c14/test/read/slices/src.psd).
Only a four-byte resource version `7` was prepended; no descriptor values were
changed. The source resource is 1050, version 6; its descriptor begins at byte
682 of the resource. The PSD's versionInfo identifies Adobe Photoshop CS6.
SHA-256: `b73d55e4bcaa2b76075909a68430ae9eba003db4f45bbc36f9f61f0f8dba0118`.
ag-psd is MIT, copyright Agamnentzar; see its
[license](https://github.com/Agamnentzar/ag-psd/blob/04d927df87130ad6d765bc63d51ade0a98d66c14/LICENSE).

Format and enum references:

- [Adobe PSD specification, slices resource](https://www.adobe.com/devnet-apps/photoshop/fileformatashtml/): v6's layer ID is mandatory for origin 1; v7/8 use a versioned descriptor.
- [ag-psd descriptor enums](https://github.com/Agamnentzar/ag-psd/blob/04d927df87130ad6d765bc63d51ade0a98d66c14/src/descriptor.ts): `layer`, `default`, `None`, `matte`, `Clr `.
- [Adobe PITerminology.h](https://github.com/AdobeDocs/photoshop-cpp-sdk/blob/7e9aca4353a3d4615e9522f47521bf840fb4388c/pluginsdk/photoshopapi/photoshop/PITerminology.h): enum IDs `Left`, `Cntr`, `Rght`, `Top `, `Mddl`, `Bttm`; the rectangle **key** `Btom` is different.
- [Adobe PIStringTerminology.h](https://github.com/AdobeDocs/photoshop-cpp-sdk/blob/7e9aca4353a3d4615e9522f47521bf840fb4388c/pluginsdk/photoshopapi/photoshop/PIStringTerminology.h): `baseline`.
- [Adobe Photoshop CS6 Help, slice cell alignment](https://helpx.adobe.com/pdf/cs6/photoshop_reference.pdf), p. 660: default/left/centre/right and default/top/middle/bottom/baseline.

The independent Photoshop fixture covers default alignment and NativeQuad
integer RGBA, including zero alpha with nonzero RGB. Nondefault alignments use
Adobe's published enum terminology and cell-alignment order; their tests check
the actual serialized enum IDs rather than only a self round trip.
