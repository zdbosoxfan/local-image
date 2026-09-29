# Portable extraction regression fixture

`comfy-portable-bcj2.7z` contains only synthetic text and generated x86-like bytes.
Its `python.exe` is test data, not a working executable. No ComfyUI or Python code
is redistributed in this fixture.

The archive was generated with the installed official 7-Zip 26.02 x64 on Windows:

```text
7z a -t7z comfy-portable-bcj2.7z ComfyUI_windows_portable -m0=BCJ2 -m1=LZMA2 -m2=LZMA2 -m3=LZMA2 -mb0:1 -mb0s1:2 -mb0s2:3
```

The synthetic layout has `ComfyUI/main.py`, `ComfyUI/folder_paths.py`, an empty
`ComfyUI/comfy` directory, and `python_embeded/python.exe`. Both `.py` files contain
`# Synthetic ComfyUI fixture. Not executable.\n`. The fake executable starts with
`Synthetic test data, not an executable.\n`, followed by 4,096 repetitions of the
bytes `90 E8 01 00 00 00 E9 02 00 00 00`.

The regression checks that metadata reports BCJ2 and runs the application's
bundled 7-Zip decoder. This catches the official portable archive's unsupported
BCJ2 failure without a multi-gigabyte download or any GPU activity.
